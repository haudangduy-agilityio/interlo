//! The audio adapter boundary: a source and a sink swap behind `CaptureSource`
//! and `AudioOutput` and the chain gives the same output on a fixture. Every
//! failure at `start`, `stop`, `write_frames` and `flush` comes back as an error.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use audio_io::capture::UnimplementedCapture;
use audio_io::{
    ring_buffer, AudioIoError, AudioOutput, CaptureError, CaptureMethod, CaptureSource,
    OutputError, RingProducer,
};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

/// 100 ms of mono audio at the target rate.
const FIXTURE_SAMPLES: usize = 1_600;
/// Ring size used unless a test names another one (a power of two, so exact).
const RING_SAMPLES: usize = 2_048;
/// A ring smaller than the fixture, so a source can overrun it.
const SMALL_RING_SAMPLES: usize = 1_024;
/// Samples a source pushes in the overrun test, far above the small ring.
const OVERRUN_SAMPLES: usize = 4_096;
/// Block size of the process-tap fake.
const TAP_BLOCK_SAMPLES: usize = 160;
/// Block size of the virtual-device fake.
const VIRTUAL_DEVICE_BLOCK_SAMPLES: usize = 480;
/// Samples the thread source pushes per block.
const THREAD_BLOCK_SAMPLES: usize = 160;
/// Pause between thread blocks, so the push overlaps the test thread.
const THREAD_BLOCK_PAUSE: Duration = Duration::from_millis(1);
/// Wait between the two ring reads after `stop`.
const QUIET_WINDOW: Duration = Duration::from_millis(50);
/// Frames the chain reads from the ring per step.
const CHAIN_READ_FRAMES: usize = 256;
/// Frames a partial-write sink takes per call.
const SINK_CHUNK_FRAMES: usize = 100;
/// A hung chain fails the test after this wait.
const HANG_GUARD: Duration = Duration::from_secs(5);

/// Deterministic ramp. Synthetic, never read from or written to a file.
fn fixture() -> Vec<f32> {
    (0..FIXTURE_SAMPLES)
        .map(|i| i as f32 / FIXTURE_SAMPLES as f32)
        .collect()
}

/// A source that pushes `samples` in blocks of `block` inside `start`.
struct FixtureSource {
    method: CaptureMethod,
    samples: Vec<f32>,
    block: usize,
    start_error: Option<CaptureError>,
    stop_error: Option<CaptureError>,
}

impl FixtureSource {
    fn tap(samples: Vec<f32>, block: usize) -> Self {
        Self {
            method: CaptureMethod::ProcessTap {
                app_id: "meeting.app".into(),
            },
            samples,
            block,
            start_error: None,
            stop_error: None,
        }
    }

    fn virtual_device(samples: Vec<f32>, block: usize) -> Self {
        Self {
            method: CaptureMethod::VirtualDevice {
                device_id: "v2".into(),
            },
            samples,
            block,
            start_error: None,
            stop_error: None,
        }
    }

    fn failing_start(mut self, error: CaptureError) -> Self {
        self.start_error = Some(error);
        self
    }

    fn failing_stop(mut self, error: CaptureError) -> Self {
        self.stop_error = Some(error);
        self
    }
}

impl CaptureSource for FixtureSource {
    fn method(&self) -> &CaptureMethod {
        &self.method
    }

    fn start(&mut self, mut sink: RingProducer) -> Result<(), CaptureError> {
        if let Some(error) = &self.start_error {
            return Err(error.clone());
        }
        for block in self.samples.chunks(self.block) {
            sink.push_slice(block);
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        match &self.stop_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

/// A source that pushes from its own thread, like a real audio callback, and
/// joins that thread in `stop`.
struct ThreadedSource {
    method: CaptureMethod,
    samples: Vec<f32>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ThreadedSource {
    fn new(samples: Vec<f32>) -> Self {
        Self {
            method: CaptureMethod::ProcessTap {
                app_id: "meeting.app".into(),
            },
            samples,
            worker: None,
        }
    }
}

impl CaptureSource for ThreadedSource {
    fn method(&self) -> &CaptureMethod {
        &self.method
    }

    fn start(&mut self, mut sink: RingProducer) -> Result<(), CaptureError> {
        let samples = self.samples.clone();
        self.worker = Some(thread::spawn(move || {
            for block in samples.chunks(THREAD_BLOCK_SAMPLES) {
                sink.push_slice(block);
                thread::sleep(THREAD_BLOCK_PAUSE);
            }
        }));
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if let Some(worker) = self.worker.take() {
            worker.join().expect("the source thread did not panic");
        }
        Ok(())
    }
}

/// What a fake sink saw. Shared, so a test can read it after the sink moved.
#[derive(Debug, Default)]
struct SinkLog {
    frames: Vec<f32>,
    flushes: usize,
    calls: usize,
}

/// A sink that records every frame it accepts, with optional faults. It locks
/// and grows a `Vec` in `write_frames`, so it fits a control-side chain only,
/// never a real-time callback test.
struct FakeSink {
    log: Arc<Mutex<SinkLog>>,
    /// Most frames taken per call; `None` takes all.
    max_per_call: Option<usize>,
    /// 1-based call number from which `write_frames` fails.
    fail_from_call: Option<(usize, OutputError)>,
    /// `write_frames` answers `Ok(0)` for every non-empty write.
    take_nothing: bool,
    flush_error: Option<OutputError>,
}

impl FakeSink {
    fn new() -> (Self, Arc<Mutex<SinkLog>>) {
        let log = Arc::new(Mutex::new(SinkLog::default()));
        (
            Self {
                log: Arc::clone(&log),
                max_per_call: None,
                fail_from_call: None,
                take_nothing: false,
                flush_error: None,
            },
            log,
        )
    }
}

impl AudioOutput for FakeSink {
    fn write_frames(&mut self, frames: &[f32]) -> Result<usize, OutputError> {
        let call = {
            let mut log = self.log.lock().unwrap();
            log.calls += 1;
            log.calls
        };
        if let Some((from, error)) = &self.fail_from_call {
            if call >= *from {
                return Err(error.clone());
            }
        }
        if self.take_nothing {
            return Ok(0);
        }
        let taken = self
            .max_per_call
            .map_or(frames.len(), |m| m.min(frames.len()));
        self.log
            .lock()
            .unwrap()
            .frames
            .extend_from_slice(&frames[..taken]);
        Ok(taken)
    }

    fn flush(&mut self) -> Result<(), OutputError> {
        self.log.lock().unwrap().flushes += 1;
        match &self.flush_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

/// The chain: start, stop, read the ring and write to the sink until the ring
/// is empty, then flush. It names only the two traits. Returns the samples the
/// ring dropped.
fn run_chain(
    source: &mut dyn CaptureSource,
    sink: &mut dyn AudioOutput,
    ring_capacity: usize,
) -> Result<u64, AudioIoError> {
    let (producer, mut consumer) = ring_buffer(ring_capacity);
    source.start(producer)?;
    source.stop()?;
    let mut scratch = vec![0.0f32; CHAIN_READ_FRAMES];
    loop {
        let read = consumer.pop_slice(&mut scratch);
        if read == 0 {
            break;
        }
        let mut rest = &scratch[..read];
        while !rest.is_empty() {
            let taken = sink.write_frames(rest)?;
            if taken == 0 {
                return Err(OutputError::Stalled.into());
            }
            rest = &rest[taken..];
        }
    }
    sink.flush()?;
    Ok(consumer.dropped_total())
}

/// Runs the chain on a helper thread and fails the test if it does not finish
/// inside `HANG_GUARD`. Returns the chain result and the frames the sink took.
fn run_chain_guarded(
    mut source: FixtureSource,
    mut sink: FakeSink,
    ring_capacity: usize,
) -> (Result<u64, AudioIoError>, usize) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = run_chain(&mut source, &mut sink, ring_capacity);
        let frames = sink.log.lock().unwrap().frames.len();
        let _ = tx.send((result, frames));
    });
    rx.recv_timeout(HANG_GUARD)
        .expect("the chain finished inside the hang guard")
}

#[test]
fn two_different_sources_give_the_same_output_on_a_fixture() {
    let sources = [
        FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES),
        FixtureSource::virtual_device(fixture(), VIRTUAL_DEVICE_BLOCK_SAMPLES),
    ];
    for mut source in sources {
        let (mut sink, log) = FakeSink::new();
        let dropped = run_chain(&mut source, &mut sink, RING_SAMPLES).unwrap();
        let log = log.lock().unwrap();
        assert_eq!(log.frames, fixture(), "{:?}", source.method());
        assert_eq!(log.flushes, 1, "{:?}", source.method());
        assert_eq!(dropped, 0, "{:?}", source.method());
    }
}

#[test]
fn two_different_sinks_give_the_same_frames_when_writes_are_partial() {
    let (mut whole, whole_log) = FakeSink::new();
    let (mut chunked, chunked_log) = FakeSink::new();
    chunked.max_per_call = Some(SINK_CHUNK_FRAMES);
    for sink in [&mut whole, &mut chunked] {
        let mut source = FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES);
        run_chain(&mut source, sink, RING_SAMPLES).unwrap();
    }
    for log in [&whole_log, &chunked_log] {
        let log = log.lock().unwrap();
        assert_eq!(log.frames, fixture());
        assert_eq!(log.flushes, 1);
    }
    assert!(chunked_log.lock().unwrap().calls > whole_log.lock().unwrap().calls);
}

#[test]
fn a_sink_error_stops_the_chain_and_no_frame_is_written_twice() {
    let (mut sink, log) = FakeSink::new();
    sink.max_per_call = Some(SINK_CHUNK_FRAMES);
    sink.fail_from_call = Some((2, OutputError::NotImplemented));
    let mut source = FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES);
    let result = run_chain(&mut source, &mut sink, RING_SAMPLES);
    assert_eq!(
        result,
        Err(AudioIoError::Output(OutputError::NotImplemented))
    );
    assert_eq!(log.lock().unwrap().frames, fixture()[..SINK_CHUNK_FRAMES]);
}

/// Checks the chain's guard against `Ok(0)`: no sink in `audio-io` returns
/// `Stalled` yet, so this covers the chain, not a production sink.
#[test]
fn a_sink_that_accepts_nothing_returns_stalled_and_does_not_hang() {
    let (mut sink, _log) = FakeSink::new();
    sink.take_nothing = true;
    let source = FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES);
    let (result, frames) = run_chain_guarded(source, sink, RING_SAMPLES);
    assert_eq!(result, Err(AudioIoError::Output(OutputError::Stalled)));
    assert_eq!(frames, 0);
}

#[test]
fn a_failing_flush_is_returned_after_every_frame_was_written() {
    let (mut sink, log) = FakeSink::new();
    sink.flush_error = Some(OutputError::NotImplemented);
    let mut source = FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES);
    let result = run_chain(&mut source, &mut sink, RING_SAMPLES);
    assert_eq!(
        result,
        Err(AudioIoError::Output(OutputError::NotImplemented))
    );
    assert_eq!(log.lock().unwrap().frames, fixture());
}

#[test]
fn a_source_that_cannot_start_returns_its_error_and_writes_nothing() {
    let (mut sink, log) = FakeSink::new();
    let mut source = FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES)
        .failing_start(CaptureError::NoCaptureRoute);
    let result = run_chain(&mut source, &mut sink, RING_SAMPLES);
    assert_eq!(
        result,
        Err(AudioIoError::Capture(CaptureError::NoCaptureRoute))
    );
    let log = log.lock().unwrap();
    assert!(log.frames.is_empty());
    assert_eq!(log.flushes, 0);
}

#[test]
fn the_placeholder_source_returns_not_implemented_and_writes_nothing() {
    let (mut sink, log) = FakeSink::new();
    let mut source = UnimplementedCapture::new(CaptureMethod::VirtualDevice {
        device_id: "v2".into(),
    });
    let result = run_chain(&mut source, &mut sink, RING_SAMPLES);
    assert_eq!(
        result,
        Err(AudioIoError::Capture(CaptureError::NotImplemented))
    );
    assert!(log.lock().unwrap().frames.is_empty());
}

#[test]
fn a_failing_stop_is_returned_and_nothing_is_written() {
    let (mut sink, log) = FakeSink::new();
    let mut source =
        FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES).failing_stop(CaptureError::NotImplemented);
    let result = run_chain(&mut source, &mut sink, RING_SAMPLES);
    assert_eq!(
        result,
        Err(AudioIoError::Capture(CaptureError::NotImplemented))
    );
    assert!(log.lock().unwrap().frames.is_empty());
}

#[test]
fn a_source_that_overruns_the_ring_loses_the_newest_samples_and_counts_them() {
    let (mut sink, log) = FakeSink::new();
    let mut source = FixtureSource::tap(fixture(), TAP_BLOCK_SAMPLES);
    let dropped = run_chain(&mut source, &mut sink, SMALL_RING_SAMPLES).unwrap();
    assert_eq!(log.lock().unwrap().frames, fixture()[..SMALL_RING_SAMPLES]);
    assert_eq!(dropped, (FIXTURE_SAMPLES - SMALL_RING_SAMPLES) as u64);
}

#[test]
fn a_push_that_exactly_fills_the_ring_loses_nothing() {
    let (mut sink, log) = FakeSink::new();
    let mut source =
        FixtureSource::tap(fixture()[..SMALL_RING_SAMPLES].to_vec(), TAP_BLOCK_SAMPLES);
    let dropped = run_chain(&mut source, &mut sink, SMALL_RING_SAMPLES).unwrap();
    assert_eq!(log.lock().unwrap().frames, fixture()[..SMALL_RING_SAMPLES]);
    assert_eq!(dropped, 0);
}

/// The fake pushes inside `start`, so this guards the ring (a full ring must
/// never block a push), not the trait signature.
#[test]
fn start_returns_while_the_consumer_has_not_read_a_single_sample() {
    let (sink, _log) = FakeSink::new();
    let source = FixtureSource::tap(vec![0.5; OVERRUN_SAMPLES], TAP_BLOCK_SAMPLES);
    let (result, frames) = run_chain_guarded(source, sink, SMALL_RING_SAMPLES);
    assert_eq!(frames, SMALL_RING_SAMPLES);
    assert_eq!(result, Ok((OVERRUN_SAMPLES - SMALL_RING_SAMPLES) as u64));
}

#[test]
fn nothing_is_pushed_after_stop_returns() {
    let (producer, consumer) = ring_buffer(RING_SAMPLES);
    let mut source = ThreadedSource::new(fixture());
    source.start(producer).unwrap();
    source.stop().unwrap();
    let first = (consumer.len(), consumer.dropped_total());
    thread::sleep(QUIET_WINDOW);
    let second = (consumer.len(), consumer.dropped_total());
    assert_eq!(first, second);
    assert_eq!(first, (FIXTURE_SAMPLES, 0));
}

#[test]
fn a_boxed_source_and_a_boxed_sink_move_to_another_thread() {
    let (sink, log) = FakeSink::new();
    let mut source: Box<dyn CaptureSource> = Box::new(ThreadedSource::new(fixture()));
    let mut sink: Box<dyn AudioOutput> = Box::new(sink);
    let worker = thread::spawn(move || run_chain(&mut *source, &mut *sink, RING_SAMPLES));
    assert_eq!(worker.join().unwrap(), Ok(0));
    assert_eq!(log.lock().unwrap().frames, fixture());
}
