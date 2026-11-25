//! The readiness probe: reads the facts that `build_report` needs from this Mac
//! (M1-41, FR-01, FR-42).
//!
//! CONTROL SIDE. The probe opens the microphone for a short time and looks at the
//! peak of each block, nothing else: no sample is copied, stored or logged
//! (guardrails 2 and 6). The block handler it hands to the backend only reads
//! atomics and a clock, so it never allocates, locks or logs (guardrail 8).
//! The sound card, the system tool and the clock sit behind traits so every rule
//! is tested with fakes.

use crate::device_list::{
    read_devices, DeviceListError, DeviceSnapshot, ProcessRunner, ToolRunner,
};
use crate::devices::DeviceInfo;
use crate::mic::{
    select_input, BlockHandler, ErrorHandler, InputBlock, MicBackend, MicConfig, SystemBackend,
};
use crate::realtime;
use crate::setup_report::{
    build_report, mic_permission_from_error, microphone, MicPermission, ModelState, SetupFacts,
    SetupFix, SetupItem, SetupItemId, SetupReport, SetupState,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

/// Absolute path, so a planted program earlier in `PATH` is never started.
const SW_VERS_PATH: &str = "/usr/bin/sw_vers";

/// Longest wait for the first block after the stream opened (starting value, to
/// confirm on a slow Mac).
pub(crate) const FIRST_BLOCK_LIMIT_MS: u64 = 4000;
/// How long the stream must carry digital silence only before it counts as
/// `NoSignal` (starting value, to confirm on a slow Mac).
pub(crate) const SILENCE_WINDOW_MS: u64 = 2000;
/// Longest a caller waits for the whole microphone probe (starting value, to
/// confirm on a slow Mac).
pub(crate) const PROBE_CALL_LIMIT_MS: u64 = 8000;
/// How often the probe looks at what the stream delivered.
const POLL_MS: u64 = 20;

/// Longest wait for `sw_vers`. Starting value.
const SW_VERS_TIMEOUT_MS: u64 = 5000;

/// Time, so a test never sleeps.
pub(crate) trait ProbeClock: Send + Sync {
    /// Called on the audio thread by the block handler: no allocation, no lock.
    fn now(&self) -> Duration;
    fn pause(&self, duration: Duration);
}

/// The real clock.
#[derive(Debug)]
pub(crate) struct SystemClock {
    start: std::time::Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbeClock for SystemClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
    fn pause(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// What the probe read, for tests of the abandon rule.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct ProbeStats {
    pub blocks_read: AtomicU64,
}

/// Shared by the probe and the block handler.
#[derive(Clone)]
pub(crate) struct ProbeCtx {
    pub clock: Arc<dyn ProbeClock>,
    pub abandon: Arc<AtomicBool>,
    #[cfg(test)]
    pub stats: Arc<ProbeStats>,
}

impl ProbeCtx {
    pub fn new(clock: Arc<dyn ProbeClock>) -> Self {
        Self {
            clock,
            abandon: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            stats: Arc::new(ProbeStats::default()),
        }
    }
}

/// The verdict at one look, or `None` while it is still open. Times are the
/// clock reading in ms; `first_block_ms` and `signal_ms` are the arrival times
/// of the first block and of the first block that carried signal.
pub(crate) fn judge(
    first_block_ms: Option<u64>,
    signal_ms: Option<u64>,
    opened_ms: u64,
    now_ms: u64,
) -> Option<MicPermission> {
    let Some(first) = first_block_ms else {
        return (now_ms.saturating_sub(opened_ms) >= FIRST_BLOCK_LIMIT_MS)
            .then_some(MicPermission::Unknown);
    };
    if first.saturating_sub(opened_ms) >= FIRST_BLOCK_LIMIT_MS {
        return Some(MicPermission::Unknown);
    }
    if signal_ms.is_some_and(|signal| signal.saturating_sub(first) < SILENCE_WINDOW_MS) {
        return Some(MicPermission::Granted);
    }
    (now_ms >= first.saturating_add(SILENCE_WINDOW_MS)).then_some(MicPermission::NoSignal)
}

/// Marks "no block seen yet" in a `Watch` timestamp.
const NONE_MS: u64 = u64::MAX;

fn ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX - 1)
}

/// What the block handler writes and the probe reads. Atomics only.
struct Watch {
    first_block_ms: AtomicU64,
    signal_ms: AtomicU64,
}

impl Watch {
    fn new() -> Self {
        Self {
            first_block_ms: AtomicU64::new(NONE_MS),
            signal_ms: AtomicU64::new(NONE_MS),
        }
    }

    /// Signal is written before the first-block time, and read in the other
    /// order, so a reader that sees a first block also sees its signal.
    fn record(&self, at_ms: u64, has_signal: bool) {
        if has_signal {
            self.signal_ms.fetch_min(at_ms, Ordering::SeqCst);
        }
        self.first_block_ms.fetch_min(at_ms, Ordering::SeqCst);
    }

    fn look(&self) -> (Option<u64>, Option<u64>) {
        let first = self.first_block_ms.load(Ordering::SeqCst);
        let signal = self.signal_ms.load(Ordering::SeqCst);
        (
            (first != NONE_MS).then_some(first),
            (signal != NONE_MS).then_some(signal),
        )
    }
}

fn block_has_signal(block: &InputBlock<'_>) -> bool {
    match block {
        InputBlock::F32(data) => data.iter().any(|s| *s != 0.0),
        InputBlock::I16(data) => data.iter().any(|s| *s != 0),
    }
}

/// Opens the microphone and decides the permission fact from what arrives.
/// Always drops the stream before it returns.
pub(crate) fn probe_microphone(
    backend: &mut dyn MicBackend,
    config: &MicConfig,
    ctx: &ProbeCtx,
) -> MicPermission {
    let devices = match backend.input_devices() {
        Ok(devices) => devices,
        Err(e) => return mic_permission_from_error(e),
    };
    let index = match select_input(&devices, config.device_name.as_deref()) {
        Ok(index) => index,
        Err(e) => return mic_permission_from_error(e),
    };
    let format = match backend.format_of(index) {
        Ok(format) => format,
        Err(e) => return mic_permission_from_error(e),
    };
    let watch = Arc::new(Watch::new());
    let on_block: BlockHandler = {
        let watch = Arc::clone(&watch);
        let clock = Arc::clone(&ctx.clock);
        let abandon = Arc::clone(&ctx.abandon);
        #[cfg(test)]
        let stats = Arc::clone(&ctx.stats);
        Box::new(move |block| {
            realtime::section(|| {
                if abandon.load(Ordering::SeqCst) {
                    return;
                }
                #[cfg(test)]
                stats.blocks_read.fetch_add(1, Ordering::SeqCst);
                watch.record(ms(clock.now()), block_has_signal(&block));
            });
        })
    };
    let on_error: ErrorHandler = Box::new(|| {});
    let stream = match backend.open(index, format, on_block, on_error) {
        Ok(stream) => stream,
        Err(e) => return mic_permission_from_error(e),
    };
    let opened_ms = ms(ctx.clock.now());
    let verdict = loop {
        if ctx.abandon.load(Ordering::SeqCst) {
            break MicPermission::Unknown;
        }
        let (first, signal) = watch.look();
        if let Some(verdict) = judge(first, signal, opened_ms, ms(ctx.clock.now())) {
            break verdict;
        }
        ctx.clock.pause(Duration::from_millis(POLL_MS));
    };
    drop(stream);
    verdict
}

/// Marks a probe worker as alive; at most one runs at a time.
#[derive(Debug, Clone, Default)]
pub struct ProbeGate {
    busy: Arc<AtomicBool>,
}

impl ProbeGate {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    fn try_acquire(&self) -> Option<GateGuard> {
        self.busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| GateGuard(Arc::clone(&self.busy)))
    }
}

/// Clears the gate when the worker ends, also when it panics.
struct GateGuard(Arc<AtomicBool>);

impl Drop for GateGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The probe on a worker thread, with a limit on how long the caller waits.
/// While an earlier worker is alive the answer is `Unknown` and nothing is
/// opened. When the limit ends first, the call is abandoned: the worker drops
/// its stream as soon as it notices and reads no further block.
pub(crate) fn probe_microphone_limited(
    gate: &ProbeGate,
    mut backend: Box<dyn MicBackend>,
    config: MicConfig,
    ctx: ProbeCtx,
    limit: Duration,
) -> MicPermission {
    let Some(guard) = gate.try_acquire() else {
        return MicPermission::Unknown;
    };
    let (sender, receiver) = mpsc::channel();
    let abandon = Arc::clone(&ctx.abandon);
    let spawned = thread::Builder::new()
        .name("mic-probe".into())
        .spawn(move || {
            let _guard = guard;
            let verdict = probe_microphone(backend.as_mut(), &config, &ctx);
            let _ = sender.send(verdict);
        });
    if spawned.is_err() {
        return MicPermission::Unknown;
    }
    match receiver.recv_timeout(limit) {
        Ok(verdict) => verdict,
        Err(_) => {
            abandon.store(true, Ordering::SeqCst);
            MicPermission::Unknown
        }
    }
}

/// Which microphone the probe opens, or `None` when there is no input device.
/// The default input is opened without a name; any other is opened by its
/// exact name.
pub(crate) fn plan_mic_open(devices: &[DeviceInfo]) -> Option<MicConfig> {
    let device = microphone(devices)?;
    Some(MicConfig {
        device_name: (!device.is_default_input).then(|| device.name.clone()),
    })
}

/// The macOS version as (major, minor): two or three numbers joined by dots.
pub(crate) fn parse_macos_version(text: &str) -> Option<(u32, u32)> {
    let parts: Vec<u32> = text
        .trim()
        .split('.')
        .map(|p| p.parse::<u32>().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [major, minor] | [major, minor, _] => Some((*major, *minor)),
        _ => None,
    }
}

/// macOS 14.4 is the floor of the process tap (FR-42, D-41).
pub(crate) fn macos_supports_tap(version: (u32, u32)) -> bool {
    version >= (14, 4)
}

/// The macOS version of this Mac as (major, minor), read with `sw_vers`.
pub fn read_system_macos_version() -> Option<(u32, u32)> {
    read_macos_version(&ProcessRunner)
}

pub(crate) fn read_macos_version(runner: &dyn ToolRunner) -> Option<(u32, u32)> {
    let bytes = runner
        .run(
            SW_VERS_PATH,
            &["-productVersion"],
            Duration::from_millis(SW_VERS_TIMEOUT_MS),
        )
        .ok()?;
    parse_macos_version(&String::from_utf8(bytes).ok()?)
}

/// Answers whether the process tap works. The real answer comes with the tap
/// capture (#73).
pub(crate) trait TapProbe {
    fn available(&self) -> bool;
}

/// The process tap is not built yet, so it is never available.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NoTapBackend;

impl TapProbe for NoTapBackend {
    fn available(&self) -> bool {
        false
    }
}

/// The tap needs macOS 14.4 or later and a working tap.
pub(crate) fn tap_available(
    read_version: impl FnOnce() -> Option<(u32, u32)>,
    tap: &dyn TapProbe,
) -> bool {
    tap.available() && read_version().is_some_and(macos_supports_tap)
}

/// Everything the probe reads from the system.
pub trait ProbeEnv {
    fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError>;
    fn probe_microphone(&mut self, config: &MicConfig) -> MicPermission;
    fn tap_available(&mut self) -> bool;
}

/// The real system: the device list tool, the microphone through the sound
/// card, `sw_vers` and the tap probe.
pub struct SystemEnv {
    gate: ProbeGate,
    tap: Box<dyn TapProbe + Send>,
}

impl SystemEnv {
    pub fn new(gate: ProbeGate) -> Self {
        Self {
            gate,
            tap: Box::new(NoTapBackend),
        }
    }
}

impl ProbeEnv for SystemEnv {
    fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
        read_devices()
    }

    fn probe_microphone(&mut self, config: &MicConfig) -> MicPermission {
        probe_microphone_limited(
            &self.gate,
            Box::new(SystemBackend::new()),
            config.clone(),
            ProbeCtx::new(Arc::new(SystemClock::new())),
            Duration::from_millis(PROBE_CALL_LIMIT_MS),
        )
    }

    fn tap_available(&mut self) -> bool {
        tap_available(read_system_macos_version, self.tap.as_ref())
    }
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub report: SetupReport,
    /// The microphone fact the report was built with; `Unknown` when the
    /// microphone was not probed or the devices could not be read.
    pub mic_permission: MicPermission,
}

/// Reads the facts: devices first, then the microphone (only when a microphone
/// device exists and the permission is not already known to be granted), then
/// the tap. Models are given.
pub(crate) fn collect_facts(
    env: &mut dyn ProbeEnv,
    models: [ModelState; 4],
    mic_known_granted: bool,
) -> Result<SetupFacts, DeviceListError> {
    let devices = env.read_devices()?;
    let mic_permission = match plan_mic_open(&devices.devices) {
        None => MicPermission::Unknown,
        Some(_) if mic_known_granted => MicPermission::Granted,
        Some(config) => env.probe_microphone(&config),
    };
    let tap_available = env.tap_available();
    Ok(SetupFacts {
        devices,
        tap_available,
        mic_permission,
        models,
    })
}

fn device_read_failed(id: SetupItemId) -> SetupItem {
    SetupItem {
        id,
        state: SetupState::Error,
        detail: "Devices cannot be read".to_string(),
        fix: Some(SetupFix::CheckAgain),
        warning: None,
    }
}

/// The report of one check. When the devices cannot be read, the four rows that
/// depend on them are `error` rows; the model rows keep their given states.
pub fn probe_setup(
    env: &mut dyn ProbeEnv,
    models: [ModelState; 4],
    mic_known_granted: bool,
) -> ProbeOutcome {
    match collect_facts(env, models, mic_known_granted) {
        Ok(facts) => ProbeOutcome {
            mic_permission: facts.mic_permission,
            report: build_report(&facts),
        },
        Err(_) => {
            let facts = SetupFacts {
                devices: DeviceSnapshot::default(),
                tap_available: false,
                mic_permission: MicPermission::Unknown,
                models,
            };
            let mut report = build_report(&facts);
            for item in report.items.iter_mut().filter(|i| {
                matches!(
                    i.id,
                    SetupItemId::Mic
                        | SetupItemId::Capture
                        | SetupItemId::Vmic
                        | SetupItemId::Phones
                )
            }) {
                *item = device_read_failed(item.id);
            }
            ProbeOutcome {
                report,
                mic_permission: MicPermission::Unknown,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{DeviceKind, Transport};
    use crate::mic::{ErrorHandler, InputDeviceEntry, SampleFormat, StreamFormat};
    use crate::setup_report::InputOpenError;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::time::Instant;

    // ---- a clock that delivers scripted blocks while time passes ----

    struct FakeInner {
        now_ms: u64,
        handler: Option<BlockHandler>,
        /// (time in ms, sample value) of each block still to come, oldest first.
        script: Vec<(u64, f32)>,
    }

    #[derive(Clone)]
    struct FakeClock {
        inner: Arc<Mutex<FakeInner>>,
    }

    impl FakeClock {
        fn new(script: &[(u64, f32)]) -> Self {
            Self {
                inner: Arc::new(Mutex::new(FakeInner {
                    now_ms: 0,
                    handler: None,
                    script: script.to_vec(),
                })),
            }
        }
        fn now_ms(&self) -> u64 {
            self.inner.lock().unwrap().now_ms
        }
        fn install(&self, handler: BlockHandler) {
            self.inner.lock().unwrap().handler = Some(handler);
        }
    }

    impl ProbeClock for FakeClock {
        fn now(&self) -> Duration {
            Duration::from_millis(self.now_ms())
        }
        fn pause(&self, duration: Duration) {
            let target = self.now_ms() + duration.as_millis() as u64;
            loop {
                let next = {
                    let mut inner = self.inner.lock().unwrap();
                    match inner.script.first().copied() {
                        Some((t, v)) if t <= target => {
                            inner.script.remove(0);
                            inner.now_ms = inner.now_ms.max(t);
                            Some((v, inner.handler.take()))
                        }
                        _ => None,
                    }
                };
                let Some((value, handler)) = next else { break };
                if let Some(mut handler) = handler {
                    handler(InputBlock::F32(&[value; 4]));
                    self.inner.lock().unwrap().handler = Some(handler);
                }
            }
            self.inner.lock().unwrap().now_ms = target;
        }
    }

    struct DropCounter(Arc<AtomicUsize>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn entry(name: &str, channels: u16, default: bool) -> InputDeviceEntry {
        InputDeviceEntry {
            name: name.into(),
            input_channels: channels,
            is_default: default,
        }
    }

    struct ScriptBackend {
        clock: FakeClock,
        open_error: Option<InputOpenError>,
        list_error: Option<InputOpenError>,
        dropped: Arc<AtomicUsize>,
        opens: Arc<AtomicUsize>,
    }

    impl ScriptBackend {
        fn new(clock: &FakeClock) -> Self {
            Self {
                clock: clock.clone(),
                open_error: None,
                list_error: None,
                dropped: Arc::new(AtomicUsize::new(0)),
                opens: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    const FORMAT: StreamFormat = StreamFormat {
        sample_rate: 48_000,
        channels: 1,
        sample: SampleFormat::F32,
    };

    impl MicBackend for ScriptBackend {
        fn input_devices(&mut self) -> Result<Vec<InputDeviceEntry>, InputOpenError> {
            match self.list_error {
                Some(e) => Err(e),
                None => Ok(vec![entry("Mic", 1, true)]),
            }
        }
        fn format_of(&mut self, _device: usize) -> Result<StreamFormat, InputOpenError> {
            Ok(FORMAT)
        }
        fn open(
            &mut self,
            _device: usize,
            _format: StreamFormat,
            on_block: BlockHandler,
            _on_error: ErrorHandler,
        ) -> Result<Box<dyn Send>, InputOpenError> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = self.open_error {
                return Err(e);
            }
            self.clock.install(on_block);
            Ok(Box::new(DropCounter(Arc::clone(&self.dropped))))
        }
    }

    /// Runs the probe on a script. Returns the verdict, the clock reading at
    /// the end and the number of stream handles dropped.
    fn run(script: &[(u64, f32)]) -> (MicPermission, u64, usize) {
        let clock = FakeClock::new(script);
        let mut backend = ScriptBackend::new(&clock);
        let ctx = ProbeCtx::new(Arc::new(clock.clone()));
        let verdict = probe_microphone(&mut backend, &MicConfig::default(), &ctx);
        (
            verdict,
            clock.now_ms(),
            backend.dropped.load(Ordering::SeqCst),
        )
    }

    // ---- AC-01 ----

    #[test]
    fn open_permission_denied_gives_denied() {
        let clock = FakeClock::new(&[]);
        let mut backend = ScriptBackend::new(&clock);
        backend.open_error = Some(InputOpenError::PermissionDenied);
        let ctx = ProbeCtx::new(Arc::new(clock));
        assert_eq!(
            probe_microphone(&mut backend, &MicConfig::default(), &ctx),
            MicPermission::Denied
        );
    }

    #[test]
    fn open_device_not_found_or_other_gives_unknown() {
        for error in [InputOpenError::DeviceNotFound, InputOpenError::Other] {
            let clock = FakeClock::new(&[]);
            let mut backend = ScriptBackend::new(&clock);
            backend.open_error = Some(error);
            let ctx = ProbeCtx::new(Arc::new(clock.clone()));
            assert_eq!(
                probe_microphone(&mut backend, &MicConfig::default(), &ctx),
                MicPermission::Unknown,
                "{error:?}"
            );
            let mut backend = ScriptBackend::new(&clock);
            backend.list_error = Some(error);
            assert_eq!(
                probe_microphone(&mut backend, &MicConfig::default(), &ctx),
                MicPermission::Unknown,
                "list {error:?}"
            );
        }
    }

    #[test]
    fn first_signal_block_gives_granted_before_the_window_ends() {
        let (verdict, end_ms, _) = run(&[(100, 0.25)]);
        assert_eq!(verdict, MicPermission::Granted);
        assert!(end_ms < 200, "decided at {end_ms} ms");
    }

    #[test]
    fn only_digital_silence_for_the_whole_window_gives_no_signal() {
        let script: Vec<(u64, f32)> = (1..=25).map(|i| (i * 100, 0.0)).collect();
        let (verdict, end_ms, _) = run(&script);
        assert_eq!(verdict, MicPermission::NoSignal);
        assert!((2100..2200).contains(&end_ms), "decided at {end_ms} ms");
    }

    #[test]
    fn no_block_within_the_first_block_limit_gives_unknown() {
        let (verdict, end_ms, _) = run(&[]);
        assert_eq!(verdict, MicPermission::Unknown);
        assert!((4000..4100).contains(&end_ms), "decided at {end_ms} ms");
    }

    #[test]
    fn limits_are_judged_at_their_boundaries() {
        assert_eq!(run(&[(3999, 0.5)]).0, MicPermission::Granted);
        assert_eq!(run(&[(4000, 0.5)]).0, MicPermission::Unknown);
        assert_eq!(run(&[(100, 0.0), (2099, 0.5)]).0, MicPermission::Granted);
        assert_eq!(run(&[(100, 0.0), (2100, 0.5)]).0, MicPermission::NoSignal);
        assert_eq!(run(&[(100, 0.0)]).0, MicPermission::NoSignal);
    }

    #[test]
    fn the_stream_is_dropped_in_every_case() {
        let scripts: [&[(u64, f32)]; 4] = [&[(100, 0.5)], &[(100, 0.0)], &[], &[(4000, 0.5)]];
        for script in scripts {
            let (_, _, dropped) = run(script);
            assert_eq!(dropped, 1, "{script:?}");
        }
    }

    #[test]
    fn judge_is_open_until_a_limit_is_reached() {
        assert_eq!(judge(None, None, 0, 3999), None);
        assert_eq!(judge(None, None, 0, 4000), Some(MicPermission::Unknown));
        assert_eq!(judge(Some(100), None, 0, 2099), None);
        assert_eq!(
            judge(Some(100), None, 0, 2100),
            Some(MicPermission::NoSignal)
        );
        assert_eq!(
            judge(Some(100), Some(100), 0, 100),
            Some(MicPermission::Granted)
        );
    }

    // ---- AC-02 and the in-flight gate ----

    /// Opens only after the test sends a release; can deliver one signal block
    /// then, and counts what it did.
    struct BlockingBackend {
        release: mpsc::Receiver<()>,
        deliver_signal: bool,
        dropped: Arc<AtomicUsize>,
        opens: Arc<AtomicUsize>,
    }

    impl MicBackend for BlockingBackend {
        fn input_devices(&mut self) -> Result<Vec<InputDeviceEntry>, InputOpenError> {
            Ok(vec![entry("Mic", 1, true)])
        }
        fn format_of(&mut self, _device: usize) -> Result<StreamFormat, InputOpenError> {
            Ok(FORMAT)
        }
        fn open(
            &mut self,
            _device: usize,
            _format: StreamFormat,
            mut on_block: BlockHandler,
            _on_error: ErrorHandler,
        ) -> Result<Box<dyn Send>, InputOpenError> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            let _ = self.release.recv();
            if self.deliver_signal {
                on_block(InputBlock::F32(&[0.5; 4]));
            }
            Ok(Box::new(DropCounter(Arc::clone(&self.dropped))))
        }
    }

    struct Blocking {
        release: mpsc::Sender<()>,
        dropped: Arc<AtomicUsize>,
        opens: Arc<AtomicUsize>,
    }

    fn blocking(deliver_signal: bool, opens: &Arc<AtomicUsize>) -> (Box<dyn MicBackend>, Blocking) {
        let (release, receiver) = mpsc::channel();
        let dropped = Arc::new(AtomicUsize::new(0));
        let backend = BlockingBackend {
            release: receiver,
            deliver_signal,
            dropped: Arc::clone(&dropped),
            opens: Arc::clone(opens),
        };
        (
            Box::new(backend),
            Blocking {
                release,
                dropped,
                opens: Arc::clone(opens),
            },
        )
    }

    /// These tests use the real clock and real threads on purpose: the abandon
    /// and `recv_timeout` rules only show with a worker thread that really waits.
    fn system_ctx() -> ProbeCtx {
        ProbeCtx::new(Arc::new(SystemClock::new()))
    }

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "timed out: {what}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_hanging_open_returns_unknown_at_the_limit() {
        let opens = Arc::new(AtomicUsize::new(0));
        let (backend, hang) = blocking(false, &opens);
        let gate = ProbeGate::new();
        let started = Instant::now();
        let verdict = probe_microphone_limited(
            &gate,
            backend,
            MicConfig::default(),
            system_ctx(),
            Duration::from_millis(200),
        );
        assert_eq!(verdict, MicPermission::Unknown);
        assert!(started.elapsed() < Duration::from_secs(2));
        let _ = hang.release.send(());
    }

    #[test]
    fn a_fast_open_inside_the_limit_gives_its_real_result() {
        let opens = Arc::new(AtomicUsize::new(0));
        let (backend, fast) = blocking(true, &opens);
        fast.release.send(()).unwrap();
        let verdict = probe_microphone_limited(
            &ProbeGate::new(),
            backend,
            MicConfig::default(),
            system_ctx(),
            Duration::from_millis(2000),
        );
        assert_eq!(verdict, MicPermission::Granted);
    }

    #[test]
    fn a_late_stream_of_an_abandoned_call_is_dropped_unread() {
        let opens = Arc::new(AtomicUsize::new(0));
        let (backend, late) = blocking(true, &opens);
        let gate = ProbeGate::new();
        let ctx = system_ctx();
        let stats = Arc::clone(&ctx.stats);
        let verdict = probe_microphone_limited(
            &gate,
            backend,
            MicConfig::default(),
            ctx,
            Duration::from_millis(50),
        );
        assert_eq!(verdict, MicPermission::Unknown);
        late.release.send(()).unwrap();
        wait_until("the late stream is dropped", || {
            late.dropped.load(Ordering::SeqCst) == 1
        });
        wait_until("the gate is clear", || !gate.is_busy());
        assert_eq!(stats.blocks_read.load(Ordering::SeqCst), 0);
        let (next, ok) = blocking(true, &opens);
        ok.release.send(()).unwrap();
        let verdict = probe_microphone_limited(
            &gate,
            next,
            MicConfig::default(),
            system_ctx(),
            Duration::from_millis(2000),
        );
        assert_eq!(verdict, MicPermission::Granted);
        assert_eq!(opens.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn an_abandoned_call_with_no_block_drops_its_stream() {
        let opens = Arc::new(AtomicUsize::new(0));
        let (backend, slow) = blocking(false, &opens);
        slow.release.send(()).unwrap();
        let gate = ProbeGate::new();
        let started = Instant::now();
        let verdict = probe_microphone_limited(
            &gate,
            backend,
            MicConfig::default(),
            system_ctx(),
            Duration::from_millis(100),
        );
        assert_eq!(verdict, MicPermission::Unknown);
        wait_until("the stream is dropped", || {
            slow.dropped.load(Ordering::SeqCst) == 1
        });
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_blocked_worker_makes_the_next_check_answer_unknown_without_a_second_open() {
        let opens = Arc::new(AtomicUsize::new(0));
        let (backend, first) = blocking(false, &opens);
        let gate = ProbeGate::new();
        let verdict = probe_microphone_limited(
            &gate,
            backend,
            MicConfig::default(),
            system_ctx(),
            Duration::from_millis(50),
        );
        assert_eq!(verdict, MicPermission::Unknown);
        wait_until("the worker is inside open", || {
            first.opens.load(Ordering::SeqCst) == 1
        });
        let (second_backend, second) = blocking(true, &opens);
        second.release.send(()).unwrap();
        let verdict = probe_microphone_limited(
            &gate,
            second_backend,
            MicConfig::default(),
            system_ctx(),
            Duration::from_millis(500),
        );
        assert_eq!(verdict, MicPermission::Unknown);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        first.release.send(()).unwrap();
        wait_until("the gate is clear", || !gate.is_busy());
    }

    // ---- AC-03 ----

    fn mic_device(name: &str, default_input: bool) -> DeviceInfo {
        DeviceInfo::new(name, name, DeviceKind::Microphone)
            .with_channels(1, 0)
            .with_transport(Transport::BuiltIn)
            .with_defaults(default_input, false)
    }

    #[test]
    fn no_input_device_opens_nothing() {
        let virtual_in = DeviceInfo::new("V", "V", DeviceKind::VirtualInput2ch)
            .with_channels(2, 2)
            .with_transport(Transport::Virtual);
        assert_eq!(plan_mic_open(&[]), None);
        assert_eq!(plan_mic_open(&[virtual_in]), None);
    }

    #[test]
    fn default_microphone_is_opened_without_a_name() {
        let config = plan_mic_open(&[mic_device("Jack", true)]).unwrap();
        assert_eq!(config.device_name, None);
    }

    #[test]
    fn other_microphone_is_opened_by_its_exact_name() {
        let virtual_default = DeviceInfo::new("V", "V", DeviceKind::VirtualInput2ch)
            .with_channels(2, 2)
            .with_transport(Transport::Virtual)
            .with_defaults(true, false);
        let config = plan_mic_open(&[virtual_default, mic_device("Desk Mic", false)]).unwrap();
        assert_eq!(config.device_name.as_deref(), Some("Desk Mic"));
    }

    // ---- AC-07 ----

    struct FakeRunner {
        output: Result<Vec<u8>, DeviceListError>,
        calls: Mutex<Vec<(String, Vec<String>, Duration)>>,
    }

    impl FakeRunner {
        fn new(output: Result<Vec<u8>, DeviceListError>) -> Self {
            Self {
                output,
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl ToolRunner for FakeRunner {
        fn run(
            &self,
            program: &str,
            args: &[&str],
            limit: Duration,
        ) -> Result<Vec<u8>, DeviceListError> {
            self.calls.lock().unwrap().push((
                program.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
                limit,
            ));
            self.output.clone()
        }
    }

    #[test]
    fn macos_version_boundaries() {
        for (text, expected) in [
            ("14.3.9", false),
            ("14.3", false),
            ("13.9", false),
            ("14.4", true),
            ("14.4.1", true),
            ("15.0", true),
            ("26.4.1\n", true),
        ] {
            let version = parse_macos_version(text).unwrap();
            assert_eq!(macos_supports_tap(version), expected, "{text:?}");
        }
    }

    #[test]
    fn unreadable_version_is_not_supported() {
        for text in ["", "abc", "14", "14.x", "14..1", ".4", "14.4.1.2"] {
            assert_eq!(parse_macos_version(text), None, "{text:?}");
        }
        for output in [
            Err(DeviceListError::ToolNotFound),
            Err(DeviceListError::TimedOut),
            Ok(vec![0xff, 0xfe]),
            Ok(b"abc\n".to_vec()),
        ] {
            assert_eq!(read_macos_version(&FakeRunner::new(output)), None);
        }
    }

    #[test]
    fn version_is_read_with_the_fixed_program_and_arguments() {
        let runner = FakeRunner::new(Ok(b"26.4.1\n".to_vec()));
        assert_eq!(read_macos_version(&runner), Some((26, 4)));
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "/usr/bin/sw_vers");
        assert_eq!(calls[0].1, ["-productVersion"]);
        assert_eq!(calls[0].2, Duration::from_millis(SW_VERS_TIMEOUT_MS));
    }

    struct FixedTap(bool);
    impl TapProbe for FixedTap {
        fn available(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn tap_needs_version_and_probe() {
        let new = Some((15, 0));
        let old = Some((14, 3));
        assert!(tap_available(|| new, &FixedTap(true)));
        assert!(!tap_available(|| new, &FixedTap(false)));
        assert!(!tap_available(|| old, &FixedTap(true)));
        assert!(!tap_available(|| None, &FixedTap(true)));
    }

    #[test]
    fn the_version_is_not_read_when_the_tap_is_not_available() {
        let reads = std::cell::Cell::new(0);
        let read = || {
            reads.set(reads.get() + 1);
            Some((15, 0))
        };
        assert!(!tap_available(read, &FixedTap(false)));
        assert_eq!(reads.get(), 0);
    }

    #[test]
    fn real_tap_probe_says_not_built() {
        assert!(!NoTapBackend.available());
        assert!(!tap_available(|| Some((26, 4)), &NoTapBackend));
    }

    // ---- AC-07 (capture row), AC-08 ----

    fn dev(
        name: &str,
        kind: DeviceKind,
        input: u32,
        output: u32,
        transport: Transport,
    ) -> DeviceInfo {
        DeviceInfo::new(name, name, kind)
            .with_channels(input, output)
            .with_transport(transport)
    }

    fn all_devices() -> DeviceSnapshot {
        DeviceSnapshot {
            devices: vec![
                dev("Mic", DeviceKind::Microphone, 1, 0, Transport::BuiltIn)
                    .with_defaults(true, false),
                dev("V2", DeviceKind::VirtualInput2ch, 2, 2, Transport::Virtual),
                dev(
                    "V16",
                    DeviceKind::VirtualOutput16ch,
                    16,
                    16,
                    Transport::Virtual,
                ),
                dev("Phones", DeviceKind::Headphones, 0, 2, Transport::BuiltIn)
                    .with_defaults(false, true),
            ],
            skipped: 0,
        }
    }

    struct FakeEnv {
        devices: Result<DeviceSnapshot, DeviceListError>,
        mic: MicPermission,
        tap: bool,
        mic_calls: Vec<MicConfig>,
        log: Vec<&'static str>,
    }

    impl FakeEnv {
        fn new(devices: Result<DeviceSnapshot, DeviceListError>) -> Self {
            Self {
                devices,
                mic: MicPermission::Granted,
                tap: false,
                mic_calls: Vec::new(),
                log: Vec::new(),
            }
        }
    }

    impl ProbeEnv for FakeEnv {
        fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
            self.log.push("devices");
            self.devices.clone()
        }
        fn probe_microphone(&mut self, config: &MicConfig) -> MicPermission {
            self.log.push("mic");
            self.mic_calls.push(config.clone());
            self.mic
        }
        fn tap_available(&mut self) -> bool {
            self.log.push("tap");
            self.tap
        }
    }

    #[test]
    fn capture_row_uses_the_two_channel_device_without_a_tap() {
        let mut env = FakeEnv::new(Ok(all_devices()));
        let outcome = probe_setup(&mut env, [ModelState::Missing; 4], false);
        let capture = &outcome.report.items[1];
        assert_eq!(capture.id, SetupItemId::Capture);
        assert_eq!(capture.state, SetupState::Ok);
        assert!(capture.detail.contains("2-channel"), "{}", capture.detail);
        let mut no_2ch = all_devices();
        no_2ch
            .devices
            .retain(|d| d.kind != DeviceKind::VirtualInput2ch);
        let mut env = FakeEnv::new(Ok(no_2ch));
        let outcome = probe_setup(&mut env, [ModelState::Missing; 4], false);
        let capture = &outcome.report.items[1];
        assert_eq!(
            (capture.state, capture.fix),
            (SetupState::Missing, Some(SetupFix::InstallCaptureDevice))
        );
    }

    #[test]
    fn facts_are_assembled_in_one_pass() {
        let mut env = FakeEnv::new(Ok(all_devices()));
        env.tap = true;
        let models = [
            ModelState::Ok,
            ModelState::Missing,
            ModelState::Downloading,
            ModelState::Error,
        ];
        let facts = collect_facts(&mut env, models, false).unwrap();
        assert_eq!(
            facts,
            SetupFacts {
                devices: all_devices(),
                tap_available: true,
                mic_permission: MicPermission::Granted,
                models,
            }
        );
        assert_eq!(env.log, ["devices", "mic", "tap"]);
        assert_eq!(env.mic_calls, [MicConfig::default()]);
    }

    #[test]
    fn a_known_granted_microphone_is_not_probed_again() {
        let mut env = FakeEnv::new(Ok(all_devices()));
        env.mic = MicPermission::Denied;
        let facts = collect_facts(&mut env, [ModelState::Ok; 4], true).unwrap();
        assert_eq!(facts.mic_permission, MicPermission::Granted);
        assert!(env.mic_calls.is_empty());
    }

    #[test]
    fn no_microphone_device_means_no_probe() {
        let mut snapshot = all_devices();
        snapshot
            .devices
            .retain(|d| d.kind != DeviceKind::Microphone);
        let mut env = FakeEnv::new(Ok(snapshot));
        let facts = collect_facts(&mut env, [ModelState::Ok; 4], false).unwrap();
        assert!(env.mic_calls.is_empty());
        assert_eq!(facts.mic_permission, MicPermission::Unknown);
    }

    #[test]
    fn a_failed_device_read_gives_error_rows_and_opens_nothing() {
        for error in [
            DeviceListError::ToolNotFound,
            DeviceListError::ToolFailed,
            DeviceListError::InvalidOutput,
            DeviceListError::TimedOut,
        ] {
            let mut env = FakeEnv::new(Err(error));
            let models = [
                ModelState::Ok,
                ModelState::Missing,
                ModelState::Downloading,
                ModelState::Error,
            ];
            let outcome = probe_setup(&mut env, models, false);
            assert!(env.mic_calls.is_empty(), "{error:?}");
            assert_eq!(outcome.mic_permission, MicPermission::Unknown);
            let ids: Vec<SetupItemId> = outcome.report.items.iter().map(|i| i.id).collect();
            assert_eq!(ids.len(), 8);
            for item in &outcome.report.items[..4] {
                assert_eq!(
                    (item.state, item.fix),
                    (SetupState::Error, Some(SetupFix::CheckAgain)),
                    "{:?} {error:?}",
                    item.id
                );
                assert_eq!(item.detail, "Devices cannot be read");
            }
            let model_states: Vec<SetupState> =
                outcome.report.items[4..].iter().map(|i| i.state).collect();
            assert_eq!(
                model_states,
                [
                    SetupState::Ok,
                    SetupState::Missing,
                    SetupState::Installing,
                    SetupState::Error
                ]
            );
        }
    }
}
