//! The speech chain of one session: samples in, captions out.
//!
//! Two worker threads. The reader takes samples from the source, runs the voice
//! activity detector, builds the spans and checks the source for a failure. The
//! recognition thread takes the spans in order, so a slow recognizer never stops
//! the reader. Waiting spans have no fixed bound.
//!
//! Every call of the sink happens under one gate lock, and `stop` closes the
//! gate under that lock: nothing is sent after `stop` returns, and `stop` does
//! not wait for a running engine call. `ChainSink` states what a sink must
//! not do.
//!
//! The chain opens no connection, writes no file and logs no text.

use crate::error::EngineError;
use crate::registry::EngineRegistry;
use crate::speech_events::{SpeechConfig, SpeechDetector, FRAME_SAMPLES};
use crate::traits::{Recognizer, Vad};
use crate::utterance::{Span, SpanBuilder};
use meeting_core::policy::PrivacyPolicy;
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard};
use std::thread;
use std::time::{Duration, Instant};

/// The source is checked for a failure this often. Starting value.
pub const CAPTURE_CHECK_MS: u64 = 200;
/// How long the reader waits when the source has no sample. Starting value.
const IDLE_WAIT: Duration = Duration::from_millis(5);
/// The most samples that one read takes.
const READ_SAMPLES: usize = 4 * FRAME_SAMPLES;
/// One call and one retry before a recognition counts as failed (FR-66).
const RECOGNITION_ATTEMPTS: u32 = 2;
/// The line that each cut at the length limit writes to the error output.
pub const FORCED_CUT_LINE: &str = "span cut at the limit";

/// Where the chain reads mono 16 kHz samples from. The chain drops the source
/// when the session ends.
pub trait FrameSource: Send {
    /// Copy ready samples to the start of `out` and return how many. Never waits.
    fn read(&mut self, out: &mut [f32]) -> usize;
    /// True once the source cannot deliver samples any more.
    fn failed(&self) -> bool;
}

/// Milliseconds from any fixed point. It never goes back.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// The clock of the running program.
#[derive(Debug, Clone)]
pub struct MonotonicClock(Instant);

impl Default for MonotonicClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Clock for MonotonicClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// The text of one span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caption {
    /// A number that no other caption of this program run has.
    pub line_id: u64,
    pub text: String,
    /// From the end of speech (or the cut) to the text.
    pub delay_ms: u64,
}

/// Nearest-rank percentiles of the delays of this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Latency {
    pub p50_ms: u64,
    pub p95_ms: u64,
}

/// Why the chain ended by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainEnd {
    /// The source reported a failure.
    SourceLost,
    /// The voice activity detector failed, a worker thread panicked, or an
    /// engine is broken by an earlier panic. The caller drops the engines.
    Failed,
}

/// Receives what one session produces. The calls run one at a time under the
/// gate of the session: a sink must not block, must not panic and must not call
/// `stop` from inside one of them.
pub trait ChainSink: Send {
    fn caption(&mut self, caption: Caption);
    fn latency(&mut self, latency: Latency);
    /// One span could not be recognized, also at the second call.
    fn recognition_failed(&mut self);
    /// The chain ended by itself. No call follows this one.
    fn ended(&mut self, end: ChainEnd);
}

/// The loaded engines of the chain, shared by every session. The detector and
/// the recognizer each sit in their own registry, behind their own lock, because
/// one registry cannot lend two engines at the same time.
pub struct Engines {
    detector: Mutex<EngineRegistry>,
    recognizer: Mutex<EngineRegistry>,
    policy: Arc<RwLock<PrivacyPolicy>>,
}

impl Engines {
    /// Register both engines. The privacy policy decides whether each may enter.
    pub fn new(
        vad: Box<dyn Vad>,
        recognizer: Box<dyn Recognizer>,
        policy: Arc<RwLock<PrivacyPolicy>>,
    ) -> Result<Self, EngineError> {
        let mut detectors = EngineRegistry::new();
        let mut recognizers = EngineRegistry::new();
        {
            let rules = read(&policy);
            detectors.set_vad(vad, &rules)?;
            recognizers.set_recognizer(recognizer, &rules)?;
        }
        Ok(Self {
            detector: Mutex::new(detectors),
            recognizer: Mutex::new(recognizers),
            policy,
        })
    }

    /// True once a call of the detector or the recognizer panicked. The caller
    /// then drops these engines and loads new ones. A panic in a call that
    /// outlives its session sends no `ended`, so ask before a new session.
    pub fn is_broken(&self) -> bool {
        self.detector.is_poisoned() || self.recognizer.is_poisoned()
    }
}

impl std::fmt::Debug for Engines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engines").finish_non_exhaustive()
    }
}

/// The gate and the queue of a session stay usable after a panic: it ends that
/// session. The engine locks are different, see `lock_registry`.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The lock of one engine. A poisoned lock means that a call of this engine
/// panicked: no session uses it again.
fn lock_registry(
    registry: &Mutex<EngineRegistry>,
) -> Result<MutexGuard<'_, EngineRegistry>, ChainEnd> {
    registry.lock().map_err(|_| ChainEnd::Failed)
}

fn read(policy: &RwLock<PrivacyPolicy>) -> RwLockReadGuard<'_, PrivacyPolicy> {
    policy.read().unwrap_or_else(PoisonError::into_inner)
}

/// A span that waits for the recognizer.
struct Waiting {
    span: Span,
    /// When speech ended, or when the span was cut.
    ended_ms: u64,
}

/// What the two threads and the owner of the session share.
struct Shared {
    /// Written only to close, and under the gate lock by `stop` and `end`.
    closed: AtomicBool,
    gate: Mutex<Box<dyn ChainSink>>,
    queue: Mutex<VecDeque<Waiting>>,
    queued: Condvar,
    pending: AtomicUsize,
}

impl Shared {
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Call the sink, unless the session is closed.
    fn send(&self, call: impl FnOnce(&mut dyn ChainSink)) {
        let mut sink = lock(&self.gate);
        if !self.is_closed() {
            call(sink.as_mut());
        }
    }

    /// Close the gate. Waits for a sink call that runs now, never for an engine.
    fn close(&self) {
        {
            let _gate = lock(&self.gate);
            self.closed.store(true, Ordering::SeqCst);
        }
        self.drop_waiting();
    }

    /// Close the gate and tell the sink why the chain ended, one time.
    fn end(&self, end: ChainEnd) {
        {
            let mut sink = lock(&self.gate);
            if self.closed.swap(true, Ordering::SeqCst) {
                return;
            }
            sink.ended(end);
        }
        self.drop_waiting();
    }

    /// Free the audio of the waiting spans and wake the recognition thread.
    fn drop_waiting(&self) {
        let mut queue = lock(&self.queue);
        self.pending.fetch_sub(queue.len(), Ordering::SeqCst);
        queue.clear();
        drop(queue);
        self.queued.notify_all();
    }

    fn add(&self, waiting: Waiting) {
        let mut queue = lock(&self.queue);
        if self.is_closed() {
            return;
        }
        self.pending.fetch_add(1, Ordering::SeqCst);
        queue.push_back(waiting);
        drop(queue);
        self.queued.notify_all();
    }

    /// The oldest waiting span, or `None` once the session is closed.
    fn next(&self) -> Option<Waiting> {
        let mut queue = lock(&self.queue);
        loop {
            if self.is_closed() {
                return None;
            }
            if let Some(waiting) = queue.pop_front() {
                return Some(waiting);
            }
            queue = self
                .queued
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// The reader thread: samples to spans until the session closes.
fn read_stream(
    shared: &Shared,
    engines: &Engines,
    mut source: Box<dyn FrameSource>,
    clock: &dyn Clock,
    started_ms: u64,
) -> Result<(), ChainEnd> {
    {
        let mut registry = lock_registry(&engines.detector)?;
        if shared.is_closed() {
            return Ok(());
        }
        // The policy lock is free again before the engine runs.
        let vad = registry.get_vad(&read(&engines.policy));
        vad.map_err(|_| ChainEnd::Failed)?.reset();
    }
    let mut detector = SpeechDetector::new(SpeechConfig::default());
    let mut spans = SpanBuilder::new();
    let mut samples = [0.0; READ_SAMPLES];
    let mut unframed: Vec<f32> = Vec::with_capacity(READ_SAMPLES + FRAME_SAMPLES);
    let mut next_check_ms = started_ms + CAPTURE_CHECK_MS;
    while !shared.is_closed() {
        let now_ms = clock.now_ms();
        if now_ms >= next_check_ms {
            if source.failed() {
                return Err(ChainEnd::SourceLost);
            }
            while next_check_ms <= now_ms {
                next_check_ms += CAPTURE_CHECK_MS;
            }
        }
        let count = source.read(&mut samples);
        if count == 0 {
            thread::sleep(IDLE_WAIT);
            continue;
        }
        unframed.extend_from_slice(&samples[..count]);
        let (frames, _) = unframed.as_chunks::<FRAME_SAMPLES>();
        let framed = frames.len() * FRAME_SAMPLES;
        for frame in frames {
            let event = {
                let mut registry = lock_registry(&engines.detector)?;
                // A closed session leaves the detector to the next session.
                if shared.is_closed() {
                    return Ok(());
                }
                // The policy lock is free again before the engine runs.
                let vad = registry.get_vad(&read(&engines.policy));
                vad.and_then(|vad| detector.push(vad, frame))
                    .map_err(|_| ChainEnd::Failed)?
            };
            for span in spans.push(frame, event) {
                if span.forced {
                    eprintln!("{FORCED_CUT_LINE}");
                }
                shared.add(Waiting {
                    span,
                    ended_ms: clock.now_ms(),
                });
            }
        }
        unframed.drain(..framed);
    }
    Ok(())
}

/// The text of one span, with the retry of FR-66. `Ok(None)`: both calls
/// failed, or the session closed.
fn recognize(
    shared: &Shared,
    engines: &Engines,
    samples: &[f32],
) -> Result<Option<String>, ChainEnd> {
    for _ in 0..RECOGNITION_ATTEMPTS {
        // A session that starts after a stop waits here for the old call.
        let mut registry = lock_registry(&engines.recognizer)?;
        if shared.is_closed() {
            return Ok(None);
        }
        // The policy lock is free again before the engine runs.
        let recognizer = registry.get_recognizer(&read(&engines.policy));
        if let Ok(recognition) = recognizer.and_then(|engine| engine.recognize(samples)) {
            return Ok(Some(recognition.text));
        }
    }
    Ok(None)
}

/// Counts one span as done when it drops, also when its call panics.
struct Done<'a>(&'a AtomicUsize);

impl Drop for Done<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The recognition thread: spans to captions, in order.
fn recognize_spans(shared: &Shared, engines: &Engines, clock: &dyn Clock) -> Result<(), ChainEnd> {
    /// Line numbers of the whole program run.
    static NEXT_LINE: AtomicU64 = AtomicU64::new(1);
    let mut delays: Vec<u64> = Vec::new();
    while let Some(Waiting { span, ended_ms }) = shared.next() {
        let _done = Done(&shared.pending);
        let text = recognize(shared, engines, &span.samples)?;
        let delay_ms = clock.now_ms().saturating_sub(ended_ms);
        // The audio of the span is not needed after its call.
        drop(span);
        match text {
            None => shared.send(|sink| sink.recognition_failed()),
            Some(text) if text.trim().is_empty() => {}
            Some(text) => {
                let at = delays.partition_point(|delay| *delay <= delay_ms);
                delays.insert(at, delay_ms);
                let latency = Latency {
                    p50_ms: nearest_rank(&delays, 50),
                    p95_ms: nearest_rank(&delays, 95),
                };
                shared.send(|sink| {
                    sink.caption(Caption {
                        line_id: NEXT_LINE.fetch_add(1, Ordering::Relaxed),
                        text,
                        delay_ms,
                    });
                    sink.latency(latency);
                });
            }
        }
    }
    Ok(())
}

/// Run one worker; a panic ends the session instead of the program.
fn guarded(shared: &Shared, work: impl FnOnce() -> Result<(), ChainEnd>) {
    match panic::catch_unwind(AssertUnwindSafe(work)) {
        Ok(Ok(())) => {}
        Ok(Err(end)) => shared.end(end),
        Err(_) => shared.end(ChainEnd::Failed),
    }
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> Result<(), EngineError> {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(work)
        .map(drop)
        .map_err(|_| EngineError::Backend("a worker thread could not start".into()))
}

/// A running session of the chain. Dropping it ends the session too.
pub struct LiveChain {
    shared: Arc<Shared>,
}

impl LiveChain {
    /// Start the two threads of one session.
    pub fn start(
        source: Box<dyn FrameSource>,
        engines: Arc<Engines>,
        clock: Arc<dyn Clock>,
        sink: Box<dyn ChainSink>,
    ) -> Result<Self, EngineError> {
        let chain = Self {
            shared: Arc::new(Shared {
                closed: AtomicBool::new(false),
                gate: Mutex::new(sink),
                queue: Mutex::new(VecDeque::new()),
                queued: Condvar::new(),
                pending: AtomicUsize::new(0),
            }),
        };
        let started_ms = clock.now_ms();
        let (shared, recognizer_engines) = (Arc::clone(&chain.shared), Arc::clone(&engines));
        let recognizer_clock = Arc::clone(&clock);
        spawn("live-recognizer", move || {
            guarded(&shared, || {
                recognize_spans(&shared, &recognizer_engines, recognizer_clock.as_ref())
            });
        })?;
        let shared = Arc::clone(&chain.shared);
        // On an error the drop of `chain` ends the thread that did start.
        spawn("live-reader", move || {
            guarded(&shared, || {
                read_stream(&shared, &engines, source, clock.as_ref(), started_ms)
            });
        })?;
        Ok(chain)
    }

    /// End the session. Nothing is sent after this call returns. Waiting spans
    /// are dropped; a running engine call is not waited for.
    pub fn stop(&self) {
        self.shared.close();
    }

    /// Spans that wait for the recognizer or are in its call.
    pub fn pending_spans(&self) -> usize {
        self.shared.pending.load(Ordering::SeqCst)
    }
}

impl Drop for LiveChain {
    /// Does not wait for a sink call that runs now: call `stop` first when
    /// nothing may follow.
    fn drop(&mut self) {
        // No gate lock here: a drop inside a sink call must not block.
        self.shared.closed.store(true, Ordering::SeqCst);
        self.shared.drop_waiting();
    }
}

impl std::fmt::Debug for LiveChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveChain").finish_non_exhaustive()
    }
}

/// The nearest-rank percentile of `sorted`, which holds at least one value.
fn nearest_rank(sorted: &[u64], percent: usize) -> u64 {
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{Engine, Recognition};
    use meeting_core::policy::Provider;
    use meeting_core::protocol::Language;
    use std::sync::mpsc::{channel, Receiver, Sender};

    /// The longest a test waits for the chain before it fails.
    const WAIT: Duration = Duration::from_secs(5);

    /// Wait until `ready` is true.
    fn until(what: &str, ready: impl Fn() -> bool) {
        let started = Instant::now();
        while !ready() {
            assert!(started.elapsed() < WAIT, "timed out: {what}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[derive(Default)]
    struct TestClock(AtomicU64);

    impl TestClock {
        fn set(&self, now_ms: u64) {
            self.0.store(now_ms, Ordering::SeqCst);
        }
    }

    impl Clock for TestClock {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    #[derive(Default)]
    struct FeedState {
        waiting: VecDeque<f32>,
        pushed: usize,
        read: usize,
        empty_reads: u64,
        failed: bool,
        released: bool,
    }

    /// The microphone of a test: the test pushes samples, the chain reads them.
    #[derive(Clone, Default)]
    struct Feed(Arc<Mutex<FeedState>>);

    impl Feed {
        fn push(&self, samples: &[f32]) {
            let mut state = self.0.lock().unwrap();
            state.waiting.extend(samples.iter().copied());
            state.pushed += samples.len();
        }

        fn fail(&self) {
            self.0.lock().unwrap().failed = true;
        }

        fn released(&self) -> bool {
            self.0.lock().unwrap().released
        }

        fn all_read(&self) -> bool {
            let state = self.0.lock().unwrap();
            state.waiting.is_empty() && state.read == state.pushed
        }

        fn empty_reads(&self) -> u64 {
            self.0.lock().unwrap().empty_reads
        }

        /// Wait until the reader has handled every pushed sample and has come
        /// back for more, `rounds` times.
        fn settle(&self, rounds: u64) {
            until("the samples are read", || self.all_read());
            let seen = self.empty_reads();
            until("the reader comes back", || {
                self.empty_reads() >= seen + rounds
            });
        }

        fn source(&self) -> Box<dyn FrameSource> {
            Box::new(FeedSource(self.clone()))
        }
    }

    struct FeedSource(Feed);

    /// The most samples that one read of the test source gives. Not a multiple
    /// of a frame, so most reads end in the middle of one.
    const FEED_READ: usize = 700;

    impl FrameSource for FeedSource {
        fn read(&mut self, out: &mut [f32]) -> usize {
            let mut state = self.0 .0.lock().unwrap();
            let count = out.len().min(state.waiting.len()).min(FEED_READ);
            for slot in &mut out[..count] {
                *slot = state.waiting.pop_front().unwrap_or_default();
            }
            state.read += count;
            if count == 0 {
                state.empty_reads += 1;
            }
            count
        }

        fn failed(&self) -> bool {
            self.0 .0.lock().unwrap().failed
        }
    }

    impl Drop for FeedSource {
        fn drop(&mut self) {
            self.0 .0.lock().unwrap().released = true;
        }
    }

    /// What the detector does at its next frame.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Detector {
        Work,
        Fail,
        Panic,
    }

    /// A frame is speech when its first sample is not zero.
    struct LevelVad {
        provider: Provider,
        mode: Arc<Mutex<Detector>>,
        /// `true` for a reset, `false` for a frame, in call order.
        calls: Arc<Mutex<Vec<bool>>>,
    }

    impl Engine for LevelVad {
        fn provider(&self) -> &Provider {
            &self.provider
        }
    }

    impl Vad for LevelVad {
        fn is_speech(&mut self, frame: &[f32]) -> Result<bool, EngineError> {
            self.calls.lock().unwrap().push(false);
            let mode = *self.mode.lock().unwrap();
            match mode {
                Detector::Work => Ok(frame[0] != 0.0),
                Detector::Fail => Err(EngineError::Backend("scripted".into())),
                Detector::Panic => panic!("scripted detector panic"),
            }
        }

        fn reset(&mut self) {
            self.calls.lock().unwrap().push(true);
        }
    }

    /// One call that the recognizer received.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Call {
        /// The first sample of the span that is not zero: it names the utterance.
        level: f32,
        samples: usize,
    }

    enum Reply {
        Text(&'static str),
        Fail,
        Panic,
    }

    /// A recognizer that reports each call and then waits for the test's answer.
    struct HeldRecognizer {
        provider: Provider,
        calls: Sender<Call>,
        replies: Receiver<Reply>,
        count: Arc<AtomicUsize>,
    }

    impl Engine for HeldRecognizer {
        fn provider(&self) -> &Provider {
            &self.provider
        }
    }

    impl Recognizer for HeldRecognizer {
        fn recognize(&mut self, samples: &[f32]) -> Result<Recognition, EngineError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            let level = samples.iter().copied().find(|s| *s != 0.0).unwrap_or(0.0);
            let _ = self.calls.send(Call {
                level,
                samples: samples.len(),
            });
            match self.replies.recv_timeout(WAIT) {
                Ok(Reply::Text(text)) => Ok(Recognition {
                    text: text.to_string(),
                    detected: Language::Vi,
                }),
                Ok(Reply::Panic) => panic!("scripted recognizer panic"),
                Ok(Reply::Fail) | Err(_) => Err(EngineError::Backend("scripted".into())),
            }
        }
    }

    /// What the sink of one session received, in order.
    #[derive(Debug, Clone, PartialEq)]
    enum Sent {
        Caption(Caption),
        Latency(u64, u64),
        RecognitionFailed,
        Ended(ChainEnd),
    }

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<Sent>>>);

    impl Recorder {
        fn all(&self) -> Vec<Sent> {
            self.0.lock().unwrap().clone()
        }

        fn captions(&self) -> Vec<Caption> {
            self.all()
                .into_iter()
                .filter_map(|sent| match sent {
                    Sent::Caption(caption) => Some(caption),
                    _ => None,
                })
                .collect()
        }

        fn latencies(&self) -> Vec<(u64, u64)> {
            self.all()
                .into_iter()
                .filter_map(|sent| match sent {
                    Sent::Latency(p50, p95) => Some((p50, p95)),
                    _ => None,
                })
                .collect()
        }

        fn wait_for(&self, count: usize) -> Vec<Sent> {
            until("the sink receives", || self.all().len() >= count);
            self.all()
        }

        fn sink(&self) -> Box<dyn ChainSink> {
            Box::new(self.clone())
        }
    }

    impl ChainSink for Recorder {
        fn caption(&mut self, caption: Caption) {
            self.0.lock().unwrap().push(Sent::Caption(caption));
        }

        fn latency(&mut self, latency: Latency) {
            self.0
                .lock()
                .unwrap()
                .push(Sent::Latency(latency.p50_ms, latency.p95_ms));
        }

        fn recognition_failed(&mut self) {
            self.0.lock().unwrap().push(Sent::RecognitionFailed);
        }

        fn ended(&mut self, end: ChainEnd) {
            self.0.lock().unwrap().push(Sent::Ended(end));
        }
    }

    /// Engines of a test with the handles that steer them.
    struct Rig {
        engines: Arc<Engines>,
        detector: Arc<Mutex<Detector>>,
        detector_calls: Arc<Mutex<Vec<bool>>>,
        calls: Receiver<Call>,
        replies: Sender<Reply>,
        recognizer_calls: Arc<AtomicUsize>,
        clock: Arc<TestClock>,
    }

    impl Rig {
        fn new() -> Self {
            let detector = Arc::new(Mutex::new(Detector::Work));
            let detector_calls = Arc::new(Mutex::new(Vec::new()));
            let (call_sender, calls) = channel();
            let (replies, reply_receiver) = channel();
            let recognizer_calls = Arc::new(AtomicUsize::new(0));
            let engines = Engines::new(
                Box::new(LevelVad {
                    provider: Provider::local("level-vad"),
                    mode: Arc::clone(&detector),
                    calls: Arc::clone(&detector_calls),
                }),
                Box::new(HeldRecognizer {
                    provider: Provider::local("held-recognizer"),
                    calls: call_sender,
                    replies: reply_receiver,
                    count: Arc::clone(&recognizer_calls),
                }),
                Arc::new(RwLock::new(PrivacyPolicy::local_only())),
            )
            .unwrap();
            Self {
                engines: Arc::new(engines),
                detector,
                detector_calls,
                calls,
                replies,
                recognizer_calls,
                clock: Arc::new(TestClock::default()),
            }
        }

        fn session(&self) -> (LiveChain, Feed, Recorder) {
            let feed = Feed::default();
            let recorder = Recorder::default();
            let clock = Arc::clone(&self.clock) as Arc<dyn Clock>;
            let chain = LiveChain::start(
                feed.source(),
                Arc::clone(&self.engines),
                clock,
                recorder.sink(),
            )
            .unwrap();
            (chain, feed, recorder)
        }

        fn next_call(&self) -> Call {
            self.calls
                .recv_timeout(WAIT)
                .expect("the recognizer is not called")
        }

        fn answer(&self, reply: Reply) {
            self.replies.send(reply).unwrap();
        }

        /// The next call is for the utterance `level`; answer it.
        fn answer_next(&self, level: f32, reply: Reply) {
            assert_eq!(self.next_call().level, level);
            self.answer(reply);
        }
    }

    /// Silence, then `frames` frames of speech at `level`, then enough silence
    /// to end it.
    fn utterance(level: f32, frames: usize) -> Vec<f32> {
        let mut samples = vec![0.0; 10 * FRAME_SAMPLES];
        samples.extend(vec![level; frames * FRAME_SAMPLES]);
        samples.extend(vec![0.0; 8 * FRAME_SAMPLES]);
        samples
    }

    #[test]
    fn each_span_gives_one_final_caption() {
        let rig = Rig::new();
        let (first, feed, sink) = rig.session();
        for (level, text) in [(1.0, "một"), (2.0, "  "), (3.0, "ba")] {
            feed.push(&utterance(level, 10));
            rig.answer_next(level, Reply::Text(text));
        }
        sink.wait_for(4);
        feed.settle(2);
        until("no span waits", || first.pending_spans() == 0);
        first.stop();
        let captions = sink.captions();
        assert_eq!(
            captions.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["một", "ba"],
            "an empty text gives no caption"
        );
        assert_eq!(sink.latencies().len(), 2);

        let (second, feed, later) = rig.session();
        feed.push(&utterance(4.0, 10));
        rig.answer_next(4.0, Reply::Text("bốn"));
        later.wait_for(2);
        second.stop();
        let mut lines: Vec<u64> = captions
            .iter()
            .chain(later.captions().iter())
            .map(|c| c.line_id)
            .collect();
        assert_eq!(lines.len(), 3);
        lines.sort_unstable();
        lines.dedup();
        assert_eq!(lines.len(), 3, "a line number is used once");
    }

    #[test]
    fn waiting_spans_run_in_order_without_blocking_capture() {
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        for level in [1.0, 2.0, 3.0] {
            feed.push(&utterance(level, 10));
        }
        // The first call is held: the reader still reads every sample.
        // Lead-in and 10 frames of speech: no sample is lost between reads.
        let whole = |level| Call {
            level,
            samples: 8_192,
        };
        assert_eq!(rig.next_call(), whole(1.0));
        feed.settle(2);
        until("three spans wait", || chain.pending_spans() == 3);
        assert_eq!(rig.recognizer_calls.load(Ordering::SeqCst), 1);
        assert_eq!(sink.all(), []);

        rig.answer(Reply::Text("một"));
        for (level, text) in [(2.0, "hai"), (3.0, "ba")] {
            assert_eq!(rig.next_call(), whole(level));
            rig.answer(Reply::Text(text));
        }
        sink.wait_for(6);
        chain.stop();
        assert_eq!(
            sink.captions()
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            ["một", "hai", "ba"]
        );
    }

    #[test]
    fn delay_counts_from_the_end_report_to_the_text() {
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        rig.clock.set(1_000);
        feed.push(&utterance(1.0, 10));
        assert_eq!(rig.next_call().level, 1.0);
        rig.clock.set(1_100);
        feed.push(&utterance(2.0, 10));
        feed.settle(2);
        until("the second span waits", || chain.pending_spans() == 2);
        rig.clock.set(1_450);
        rig.answer(Reply::Text("một"));
        assert_eq!(rig.next_call().level, 2.0);
        rig.clock.set(2_000);
        rig.answer(Reply::Text("hai"));
        sink.wait_for(4);

        // A forced cut: the time counts from the cut, not from the start of the
        // span, while speech goes on.
        let mut speech = vec![0.0; 10 * FRAME_SAMPLES];
        speech.extend(vec![3.0; 100 * FRAME_SAMPLES]);
        feed.push(&speech);
        feed.settle(2);
        rig.clock.set(5_000);
        feed.push(&vec![3.0; 380 * FRAME_SAMPLES]);
        let cut = rig.next_call();
        assert_eq!((cut.level, cut.samples), (3.0, 240_000));
        rig.clock.set(5_600);
        rig.answer(Reply::Text("ba"));
        sink.wait_for(6);
        chain.stop();
        assert_eq!(
            sink.captions()
                .iter()
                .map(|c| c.delay_ms)
                .collect::<Vec<_>>(),
            [450, 900, 600]
        );
    }

    #[test]
    fn latency_uses_nearest_rank_over_the_session() {
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        // Not in rising order: the percentiles are taken over sorted delays.
        for (index, delay) in [400_u64, 100, 300, 200].into_iter().enumerate() {
            let base = 1_000 * (index as u64 + 1);
            rig.clock.set(base);
            feed.push(&utterance(1.0, 10));
            assert_eq!(rig.next_call().level, 1.0);
            rig.clock.set(base + delay);
            rig.answer(Reply::Text("câu"));
            sink.wait_for(2 * (index + 1));
        }
        chain.stop();
        assert_eq!(
            sink.latencies(),
            [(400, 400), (100, 400), (300, 400), (200, 400)]
        );

        let (chain, feed, later) = rig.session();
        rig.clock.set(10_000);
        feed.push(&utterance(1.0, 10));
        assert_eq!(rig.next_call().level, 1.0);
        rig.clock.set(10_700);
        rig.answer(Reply::Text("câu"));
        later.wait_for(2);
        chain.stop();
        assert_eq!(later.latencies(), [(700, 700)]);
    }

    #[test]
    fn nearest_rank_takes_the_value_at_the_rounded_up_rank() {
        let values: Vec<u64> = (1..=20).map(|n| n * 10).collect();
        assert_eq!(nearest_rank(&values, 50), 100);
        assert_eq!(nearest_rank(&values, 95), 190);
        assert_eq!(nearest_rank(&values, 96), 200);
        assert_eq!(nearest_rank(&[7], 50), 7);
        assert_eq!(nearest_rank(&[7], 95), 7);
    }

    #[test]
    fn failed_recognition_is_retried_once() {
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        feed.push(&utterance(1.0, 10));
        let first = rig.next_call();
        rig.answer(Reply::Fail);
        assert_eq!(rig.next_call(), first, "the same span is sent again");
        rig.answer(Reply::Text("một"));
        let sent = sink.wait_for(2);
        assert!(matches!(&sent[0], Sent::Caption(c) if c.text == "một"));

        feed.push(&utterance(2.0, 10));
        rig.answer_next(2.0, Reply::Fail);
        rig.answer_next(2.0, Reply::Fail);
        let sent = sink.wait_for(3);
        assert_eq!(sent[2], Sent::RecognitionFailed);

        // No third call: the next one belongs to the next span.
        feed.push(&utterance(3.0, 10));
        rig.answer_next(3.0, Reply::Text("ba"));
        let sent = sink.wait_for(5);
        chain.stop();
        assert!(matches!(&sent[3], Sent::Caption(c) if c.text == "ba"));
        assert_eq!(rig.recognizer_calls.load(Ordering::SeqCst), 5);
        assert_eq!(
            sent.iter()
                .filter(|s| **s == Sent::RecognitionFailed)
                .count(),
            1
        );
    }

    #[test]
    fn the_reader_resets_the_detector_before_the_first_frame() {
        let rig = Rig::new();
        for _ in 0..2 {
            let (chain, feed, _sink) = rig.session();
            feed.push(&vec![0.0; 4 * FRAME_SAMPLES]);
            feed.settle(2);
            chain.stop();
            until("the source is released", || feed.released());
        }
        let calls = rig.detector_calls.lock().unwrap().clone();
        let session = [true, false, false, false, false];
        assert_eq!(calls, [session, session].concat());
    }

    #[test]
    fn engines_refuse_a_remote_provider() {
        use crate::mock::Mock;
        let policy = Arc::new(RwLock::new(PrivacyPolicy::local_only()));
        let text = || Recognition {
            text: "xin".into(),
            detected: Language::Vi,
        };
        let remote_recognizer = Mock::new(text()).remote("far-away");
        let calls = remote_recognizer.calls();
        let result = Engines::new(
            Box::new(Mock::new(true)),
            Box::new(remote_recognizer),
            Arc::clone(&policy),
        );
        assert!(matches!(result, Err(EngineError::Denied { .. })));
        assert_eq!(calls.count(), 0);
        let remote_detector = Mock::new(true).remote("far-away");
        let result = Engines::new(
            Box::new(remote_detector),
            Box::new(Mock::new(text())),
            policy,
        );
        assert!(matches!(result, Err(EngineError::Denied { .. })));
    }

    #[test]
    fn stop_sends_nothing_more_and_drops_waiting_spans() {
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        feed.push(&utterance(1.0, 10));
        feed.push(&utterance(2.0, 10));
        assert_eq!(rig.next_call().level, 1.0);
        until("one span waits", || chain.pending_spans() == 2);

        // The call is still held: stop returns without waiting for it.
        let chain = Arc::new(chain);
        let (done, stopped) = channel();
        let stopper = Arc::clone(&chain);
        thread::spawn(move || {
            stopper.stop();
            let _ = done.send(());
        });
        stopped.recv_timeout(WAIT).expect("stop waits for the call");
        until("the source is released", || feed.released());

        // A new session waits for the old call.
        let (next, next_feed, next_sink) = rig.session();
        next_feed.push(&utterance(3.0, 10));
        until("the new span waits", || next.pending_spans() == 1);
        assert!(rig.calls.try_recv().is_err());

        rig.answer(Reply::Text("muộn"));
        // The waiting span of the stopped session is never recognized: the next
        // call belongs to the new session.
        rig.answer_next(3.0, Reply::Text("mới"));
        next_sink.wait_for(2);
        next.stop();
        // The old thread has passed its chance to send.
        until("the old call is done", || chain.pending_spans() == 0);
        assert_eq!(sink.all(), [], "nothing follows a stop");
        assert_eq!(next_sink.captions()[0].text, "mới");
    }

    #[test]
    fn lost_source_ends_the_chain_at_the_next_check() {
        let rig = Rig::new();
        // The checks count from the start of the session: 1300, 1500, ...
        rig.clock.set(1_100);
        let (_chain, feed, sink) = rig.session();
        feed.settle(2);
        // A check that finds no failure.
        rig.clock.set(1_100 + CAPTURE_CHECK_MS);
        feed.settle(3);
        feed.fail();
        rig.clock.set(1_099 + 2 * CAPTURE_CHECK_MS);
        feed.settle(3);
        assert_eq!(sink.all(), [], "the check is not due yet");
        rig.clock.set(1_100 + 2 * CAPTURE_CHECK_MS);
        assert_eq!(sink.wait_for(1), [Sent::Ended(ChainEnd::SourceLost)]);
        until("the source is released", || feed.released());
        assert_eq!(CAPTURE_CHECK_MS, 200);
    }

    #[test]
    fn detector_error_ends_the_chain() {
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        feed.push(&utterance(1.0, 10));
        assert_eq!(rig.next_call().level, 1.0);
        *rig.detector.lock().unwrap() = Detector::Fail;
        feed.push(&vec![0.0; FRAME_SAMPLES]);
        assert_eq!(sink.wait_for(1), [Sent::Ended(ChainEnd::Failed)]);
        until("the source is released", || feed.released());
        // The held call returns after the end and sends nothing.
        rig.answer(Reply::Text("muộn"));
        until("the call is done", || chain.pending_spans() == 0);
        chain.stop();
        assert_eq!(sink.all(), [Sent::Ended(ChainEnd::Failed)]);
    }

    #[test]
    fn a_panic_after_stop_marks_the_engines_broken() {
        // The recognizer panics in a call that outlives its session.
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        feed.push(&utterance(1.0, 10));
        assert_eq!(rig.next_call().level, 1.0);
        chain.stop();
        assert!(!rig.engines.is_broken());
        rig.answer(Reply::Panic);
        until("the engines are broken", || rig.engines.is_broken());
        // Both threads have passed their chance to send.
        until("the old threads are done", || {
            Arc::strong_count(&chain.shared) == 1
        });
        assert_eq!(sink.all(), [], "nothing follows a stop");

        // The next session on these engines ends at its first span.
        let (_next, feed, later) = rig.session();
        feed.push(&utterance(2.0, 10));
        assert_eq!(later.wait_for(1), [Sent::Ended(ChainEnd::Failed)]);
        assert_eq!(rig.recognizer_calls.load(Ordering::SeqCst), 1);

        // After a detector panic, a session ends before its first frame.
        let rig = Rig::new();
        let (_chain, feed, sink) = rig.session();
        *rig.detector.lock().unwrap() = Detector::Panic;
        feed.push(&vec![0.0; FRAME_SAMPLES]);
        assert_eq!(sink.wait_for(1), [Sent::Ended(ChainEnd::Failed)]);
        assert!(rig.engines.is_broken());
        *rig.detector.lock().unwrap() = Detector::Work;
        let seen = rig.detector_calls.lock().unwrap().len();
        let (_next, _feed, later) = rig.session();
        assert_eq!(later.wait_for(1), [Sent::Ended(ChainEnd::Failed)]);
        assert_eq!(
            rig.detector_calls.lock().unwrap().len(),
            seen,
            "a broken detector is not called"
        );
    }

    #[test]
    fn a_panic_in_a_worker_ends_the_chain_not_the_process() {
        // The recognizer panics.
        let rig = Rig::new();
        let (chain, feed, sink) = rig.session();
        feed.push(&utterance(1.0, 10));
        rig.answer_next(1.0, Reply::Panic);
        assert_eq!(sink.wait_for(1), [Sent::Ended(ChainEnd::Failed)]);
        until("the source is released", || feed.released());
        until("no span waits", || chain.pending_spans() == 0);

        // The detector panics.
        let rig = Rig::new();
        let (_chain, feed, sink) = rig.session();
        *rig.detector.lock().unwrap() = Detector::Panic;
        feed.push(&vec![0.0; FRAME_SAMPLES]);
        assert_eq!(sink.wait_for(1), [Sent::Ended(ChainEnd::Failed)]);
        until("the source is released", || feed.released());
    }
}
