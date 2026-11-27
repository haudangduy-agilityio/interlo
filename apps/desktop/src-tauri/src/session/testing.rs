//! Fakes of the tests of the session: model files, the hasher and the computer.

use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use audio_io::{
    DeviceInfo, DeviceKind, DeviceListError, DeviceSnapshot, MicConfig, MicPermission, ProbeEnv,
    Transport,
};
use engine_host::live_chain::{Clock, FrameSource, MonotonicClock};
use engine_host::traits::{Engine, Recognition, Recognizer, Vad};
use engine_host::{EngineError, FRAME_SAMPLES};
use meeting_core::policy::Provider;
use meeting_core::protocol::Language;
use model_store::{FileDigest, Hasher, ModelFile, Pack, Sha256Hasher};

use crate::commands::setup::{SetupAnswer, SetupCheck};
use crate::events::{BannerEvent, CaptionEvent, LatencyEvent, RunState, SessionSink, StatusSink};
use crate::ipc::{
    IpcResult, MeetingHandle, SetupItemId, SetupState, SetupStatus, StartChecks, StartMeetingArgs,
};
use crate::models_dir::ModelFolder;
use crate::session::capture::{Capture, Microphone, OpenFailed};
use crate::session::machine::{LoadedEngines, Machine};
use crate::session::model_check::ModelCheck;
use crate::session::policy::shared_policy;
use crate::session::Session;

/// SHA-256 of the bytes `abc`, the content of every test model file.
const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

const fn file(rel_path: &'static str) -> ModelFile {
    ModelFile {
        rel_path,
        url: "https://models.example.com/file",
        sha256_hex: ABC_SHA256,
        size_bytes: 3,
    }
}

/// The model table of the tests: the three packs of the real table, with small
/// files.
pub static PACKS: [Pack; 3] = [
    Pack {
        id: "asr",
        files: &[file("a.bin"), file("vad/vad.onnx")],
    },
    Pack {
        id: "translation",
        files: &[file("t.bin")],
    },
    Pack {
        id: "voice",
        files: &[file("v.bin"), file("sub/w.bin")],
    },
];

/// Every file of `PACKS` as `<pack>/<file>`.
pub const FILES: [&str; 5] = [
    "asr/a.bin",
    "asr/vad/vad.onnx",
    "translation/t.bin",
    "voice/v.bin",
    "voice/sub/w.bin",
];

/// A model folder with the files of `PACKS`, removed on drop.
pub struct Folder(PathBuf);

impl Folder {
    pub fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("interlo-session-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for name in FILES {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"abc").unwrap();
        }
        Self(root)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn at(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }

    pub fn source(&self) -> ModelFolder {
        ModelFolder::At(self.0.clone())
    }

    /// The owner of the files this test made.
    pub fn uid(&self) -> u32 {
        std::fs::metadata(&self.0).unwrap().uid()
    }

    /// The file number of one file, as the recording hasher reports it.
    pub fn number(&self, relative: &str) -> u64 {
        std::fs::metadata(self.at(relative)).unwrap().ino()
    }

    /// A model check of this folder that has started.
    pub fn check(&self) -> ModelCheck {
        ModelCheck::launch(self.source(), &PACKS, self.uid(), Box::new(Sha256Hasher))
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Records the file number of every file that is hashed.
#[derive(Clone, Default)]
pub struct Recording(Arc<Mutex<Vec<u64>>>);

impl Recording {
    pub fn hashed(&self) -> Vec<u64> {
        self.0.lock().unwrap().clone()
    }
}

impl Hasher for Recording {
    fn digest(&self, file: &mut File, cap_bytes: u64) -> io::Result<FileDigest> {
        self.0.lock().unwrap().push(file.metadata()?.ino());
        Sha256Hasher.digest(file, cap_bytes)
    }
}

fn device(name: &str, kind: DeviceKind, input: u32, output: u32, how: Transport) -> DeviceInfo {
    DeviceInfo::new(name, name, kind)
        .with_channels(input, output)
        .with_transport(how)
}

/// A computer with everything a session needs: a built-in default microphone,
/// both virtual devices and headphones as the default output.
pub fn working_devices() -> Vec<DeviceInfo> {
    vec![
        device("Mic", DeviceKind::Microphone, 1, 0, Transport::BuiltIn).with_defaults(true, false),
        device("V2", DeviceKind::VirtualInput2ch, 2, 2, Transport::Virtual),
        device(
            "V16",
            DeviceKind::VirtualOutput16ch,
            16,
            16,
            Transport::Virtual,
        ),
        device("Phones", DeviceKind::Headphones, 0, 2, Transport::BuiltIn)
            .with_defaults(false, true),
    ]
}

/// One probe of the fake computer.
struct Probe<'a> {
    devices: Vec<DeviceInfo>,
    permission: MicPermission,
    probes: &'a AtomicUsize,
}

impl ProbeEnv for Probe<'_> {
    fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
        Ok(DeviceSnapshot {
            devices: self.devices.clone(),
            skipped: 0,
        })
    }

    fn probe_microphone(&mut self, _config: &MicConfig) -> MicPermission {
        self.probes.fetch_add(1, Ordering::SeqCst);
        self.permission
    }

    fn tap_available(&mut self) -> bool {
        false
    }
}

/// The longest a test waits before it fails.
pub const WAIT: Duration = Duration::from_secs(5);

/// Wait until `ready` is true.
pub fn until(what: &str, ready: impl Fn() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(started.elapsed() < WAIT, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// What happened, in order, across the fake computer and the events.
#[derive(Clone, Default)]
pub struct Journal(Arc<Mutex<Vec<String>>>);

impl Journal {
    pub fn add(&self, entry: impl Into<String>) {
        self.0.lock().unwrap().push(entry.into());
    }

    pub fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

/// Stops the calls that pass it while it is held.
#[derive(Default)]
pub struct Gate {
    state: Mutex<(bool, usize)>,
    changed: Condvar,
}

impl Gate {
    pub fn hold(&self) {
        self.state.lock().unwrap().0 = true;
    }

    pub fn release(&self) {
        self.state.lock().unwrap().0 = false;
        self.changed.notify_all();
    }

    /// Calls that have reached the gate, passed ones included.
    pub fn arrived(&self) -> usize {
        self.state.lock().unwrap().1
    }

    pub fn wait_for_arrival(&self, count: usize) {
        until("a call reaches the gate", || self.arrived() >= count);
    }

    fn pass(&self) {
        let mut state = self.state.lock().unwrap();
        state.1 += 1;
        let (state, waited) = self
            .changed
            .wait_timeout_while(state, WAIT, |state| state.0)
            .unwrap();
        drop(state);
        assert!(!waited.timed_out(), "a gate was never released");
    }
}

/// How the next load goes wrong. A load without one works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    Fail,
    Panic,
    Remote,
}

/// What the voice detector or the recognizer does at its next call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Work,
    /// Hears no speech, or recognizes no words.
    Silent,
    Fail,
    Panic,
}

/// The fake engines and what they received. Shared by every load of one test.
#[derive(Default)]
pub struct EngineControls {
    pub detector: Mutex<Mode>,
    /// `true` for a reset, `false` for a frame, in call order.
    pub detector_calls: Mutex<Vec<bool>>,
    pub recognizer: Mutex<Mode>,
    pub recognizer_gate: Gate,
    pub recognizer_calls: AtomicUsize,
}

struct FakeVad {
    provider: Provider,
    controls: Arc<EngineControls>,
}

impl Engine for FakeVad {
    fn provider(&self) -> &Provider {
        &self.provider
    }
}

/// A frame is speech when its first sample is not zero.
impl Vad for FakeVad {
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool, EngineError> {
        self.controls.detector_calls.lock().unwrap().push(false);
        let mode = *self.controls.detector.lock().unwrap();
        match mode {
            Mode::Work => Ok(frame[0] != 0.0),
            Mode::Silent => Ok(false),
            Mode::Fail => Err(EngineError::Backend("scripted".into())),
            Mode::Panic => panic!("scripted detector panic"),
        }
    }

    fn reset(&mut self) {
        self.controls.detector_calls.lock().unwrap().push(true);
    }
}

struct FakeRecognizer {
    provider: Provider,
    controls: Arc<EngineControls>,
}

impl Engine for FakeRecognizer {
    fn provider(&self) -> &Provider {
        &self.provider
    }
}

/// Answers `text <n>` for its n-th call.
impl Recognizer for FakeRecognizer {
    fn recognize(&mut self, _samples: &[f32]) -> Result<Recognition, EngineError> {
        let call = self
            .controls
            .recognizer_calls
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        self.controls.recognizer_gate.pass();
        let mode = *self.controls.recognizer.lock().unwrap();
        match mode {
            Mode::Work => Ok(Recognition {
                text: format!("text {call}"),
                detected: Language::Vi,
            }),
            Mode::Silent => Ok(Recognition {
                text: String::new(),
                detected: Language::Vi,
            }),
            Mode::Fail => Err(EngineError::Backend("scripted".into())),
            Mode::Panic => panic!("scripted recognizer panic"),
        }
    }
}

#[derive(Default)]
struct FeedState {
    waiting: VecDeque<f32>,
    failed: bool,
}

/// The samples of one opened microphone: the test pushes, the chain reads.
#[derive(Clone, Default)]
pub struct Feed(Arc<Mutex<FeedState>>);

impl Feed {
    /// Speech long enough for one span, then the silence that ends it.
    pub fn utterance(&self) {
        let mut state = self.0.lock().unwrap();
        state.waiting.extend([0.5_f32; 10 * FRAME_SAMPLES]);
        state.waiting.extend([0.0_f32; 10 * FRAME_SAMPLES]);
    }

    pub fn fail(&self) {
        self.0.lock().unwrap().failed = true;
    }
}

impl FrameSource for Feed {
    fn read(&mut self, out: &mut [f32]) -> usize {
        let mut state = self.0.lock().unwrap();
        let count = out.len().min(state.waiting.len());
        for slot in &mut out[..count] {
            *slot = state.waiting.pop_front().unwrap_or_default();
        }
        count
    }

    fn failed(&self) -> bool {
        self.0.lock().unwrap().failed
    }
}

/// What a test runs while a microphone closes.
pub type WhileClosing = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

struct FakeCapture {
    stopped: Arc<AtomicUsize>,
    while_closing: WhileClosing,
    journal: Journal,
}

impl Capture for FakeCapture {
    fn stop(&mut self) {
        let run = self.while_closing.lock().unwrap().take();
        if let Some(run) = run {
            run();
        }
        self.stopped.fetch_add(1, Ordering::SeqCst);
        self.journal.add("capture stopped");
    }
}

/// The fake computer and its controls. It runs the real setup check on fake
/// devices.
pub struct Fake {
    setup: SetupCheck,
    pub devices: Mutex<Vec<DeviceInfo>>,
    pub permission: Mutex<MicPermission>,
    /// How many of the next setup answers say that the microphone check waited.
    pub waits: AtomicUsize,
    pub setup_gate: Gate,
    /// What `meeting_running` answered at each setup check.
    pub asked: Mutex<Vec<bool>>,
    /// How often the setup check opened the microphone.
    pub probes: AtomicUsize,
    /// What goes wrong at the next loads, in order.
    pub next_loads: Mutex<VecDeque<Load>>,
    pub load_gate: Gate,
    /// The folder of each load.
    pub loads: Mutex<Vec<PathBuf>>,
    pub engines: Arc<EngineControls>,
    pub open_fails: AtomicBool,
    /// The next setup check panics.
    pub setup_panics: AtomicBool,
    /// The next opening of a microphone panics.
    pub open_panics: AtomicBool,
    pub open_gate: Gate,
    /// Runs inside the next stop of a microphone.
    pub while_closing: WhileClosing,
    /// How each microphone was asked for.
    pub opened: Mutex<Vec<MicConfig>>,
    pub feeds: Mutex<Vec<Feed>>,
    pub stopped: Arc<AtomicUsize>,
    pub journal: Journal,
}

impl Fake {
    pub fn new(journal: Journal) -> Arc<Self> {
        Arc::new(Self {
            setup: SetupCheck::default(),
            devices: Mutex::new(working_devices()),
            permission: Mutex::new(MicPermission::Granted),
            waits: AtomicUsize::new(0),
            setup_gate: Gate::default(),
            asked: Mutex::default(),
            probes: AtomicUsize::new(0),
            next_loads: Mutex::default(),
            load_gate: Gate::default(),
            loads: Mutex::default(),
            engines: Arc::default(),
            open_fails: AtomicBool::new(false),
            setup_panics: AtomicBool::new(false),
            open_panics: AtomicBool::new(false),
            open_gate: Gate::default(),
            while_closing: WhileClosing::default(),
            opened: Mutex::default(),
            feeds: Mutex::default(),
            stopped: Arc::default(),
            journal,
        })
    }

    /// The feed of the n-th opened microphone, from 0.
    pub fn feed(&self, index: usize) -> Feed {
        until("the microphone is opened", || {
            self.feeds.lock().unwrap().len() > index
        });
        self.feeds.lock().unwrap()[index].clone()
    }

    /// Microphones that were opened and are not stopped.
    pub fn open_captures(&self) -> usize {
        self.feeds.lock().unwrap().len() - self.stopped.load(Ordering::SeqCst)
    }
}

/// The `Machine` of a test.
pub struct FakeMac(pub Arc<Fake>);

impl Machine for FakeMac {
    fn setup(&self, meeting_running: &dyn Fn() -> bool) -> SetupAnswer {
        let fake = &self.0;
        fake.setup_gate.pass();
        if fake.setup_panics.swap(false, Ordering::SeqCst) {
            panic!("scripted setup panic");
        }
        let running = meeting_running();
        fake.asked.lock().unwrap().push(running);
        let waits = fake.waits.load(Ordering::SeqCst) > 0;
        if waits {
            fake.waits.fetch_sub(1, Ordering::SeqCst);
        }
        let mut probe = Probe {
            devices: fake.devices.lock().unwrap().clone(),
            permission: *fake.permission.lock().unwrap(),
            probes: &fake.probes,
        };
        fake.setup.answer(&mut probe, &|| waits || running)
    }

    fn engines(&self, folder: &Path) -> Result<LoadedEngines, EngineError> {
        let fake = &self.0;
        fake.journal.add("load start");
        fake.loads.lock().unwrap().push(folder.to_path_buf());
        let plan = fake.next_loads.lock().unwrap().pop_front();
        fake.load_gate.pass();
        fake.journal.add("load end");
        let provider = |name| match plan {
            Some(Load::Remote) => Provider::remote(name),
            _ => Provider::local(name),
        };
        match plan {
            Some(Load::Fail) => Err(EngineError::Backend(format!(
                "cannot read {}",
                folder.display()
            ))),
            Some(Load::Panic) => panic!("scripted loader panic"),
            _ => Ok(LoadedEngines {
                vad: Box::new(FakeVad {
                    provider: provider("fake-vad"),
                    controls: Arc::clone(&fake.engines),
                }),
                recognizer: Box::new(FakeRecognizer {
                    provider: provider("fake-recognizer"),
                    controls: Arc::clone(&fake.engines),
                }),
            }),
        }
    }

    fn open_microphone(&self, config: MicConfig) -> Result<Microphone, OpenFailed> {
        let fake = &self.0;
        fake.journal.add("open");
        fake.opened.lock().unwrap().push(config);
        fake.open_gate.pass();
        if fake.open_panics.swap(false, Ordering::SeqCst) {
            panic!("scripted open panic");
        }
        if fake.open_fails.load(Ordering::SeqCst) {
            return Err(OpenFailed);
        }
        let feed = Feed::default();
        fake.feeds.lock().unwrap().push(feed.clone());
        Ok(Microphone {
            source: Box::new(feed),
            capture: Box::new(FakeCapture {
                stopped: Arc::clone(&fake.stopped),
                while_closing: Arc::clone(&fake.while_closing),
                journal: fake.journal.clone(),
            }),
        })
    }
}

/// One event that the session sent to the webview.
#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    Status(RunState),
    Caption(CaptionEvent),
    Latency(LatencyEvent),
    Banner(BannerEvent),
}

/// The events of a test, in order.
#[derive(Clone)]
pub struct Events {
    sent: Arc<Mutex<Vec<Sent>>>,
    journal: Journal,
}

impl Events {
    pub fn new(journal: Journal) -> Self {
        Self {
            sent: Arc::default(),
            journal,
        }
    }

    pub fn all(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }

    pub fn statuses(&self) -> Vec<RunState> {
        self.all()
            .into_iter()
            .filter_map(|sent| match sent {
                Sent::Status(state) => Some(state),
                _ => None,
            })
            .collect()
    }

    pub fn captions(&self) -> Vec<CaptionEvent> {
        self.all()
            .into_iter()
            .filter_map(|sent| match sent {
                Sent::Caption(caption) => Some(caption),
                _ => None,
            })
            .collect()
    }

    pub fn banners(&self) -> Vec<BannerEvent> {
        self.all()
            .into_iter()
            .filter_map(|sent| match sent {
                Sent::Banner(banner) => Some(banner),
                _ => None,
            })
            .collect()
    }

    fn push(&self, sent: Sent) {
        self.sent.lock().unwrap().push(sent);
    }
}

impl StatusSink for Events {
    fn emit_status(&self, state: RunState) {
        self.journal.add(format!("status {state:?}"));
        self.push(Sent::Status(state));
    }
}

impl SessionSink for Events {
    fn emit_caption(&self, event: CaptionEvent) {
        self.push(Sent::Caption(event));
    }

    fn emit_latency(&self, event: LatencyEvent) {
        self.push(Sent::Latency(event));
    }

    fn emit_banner(&self, event: BannerEvent) {
        self.push(Sent::Banner(event));
    }
}

/// A clock that moves only when the test moves it.
#[derive(Default)]
pub struct ManualClock(AtomicU64);

impl ManualClock {
    pub fn set(&self, now_ms: u64) {
        self.0.store(now_ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// A session on the fake computer with this model check.
pub fn session_with(models: ModelCheck) -> Session {
    let machine = FakeMac(Fake::new(Journal::default()));
    let clock = Arc::new(MonotonicClock::default());
    Session::new(Box::new(machine), models, shared_policy(), clock)
}

/// A session with its model folder, its fake computer and its events.
pub struct Rig {
    pub folder: Folder,
    pub hashes: Recording,
    pub fake: Arc<Fake>,
    pub session: Session,
    pub events: Events,
    next_id: AtomicUsize,
}

impl Rig {
    pub fn new() -> Self {
        Self::timed(Arc::new(MonotonicClock::default()))
    }

    /// A rig whose session reads the time from `clock`.
    pub fn timed(clock: Arc<dyn Clock>) -> Self {
        let folder = Folder::new();
        let hashes = Recording::default();
        let models = ModelCheck::launch(
            folder.source(),
            &PACKS,
            folder.uid(),
            Box::new(hashes.clone()),
        );
        let journal = Journal::default();
        let fake = Fake::new(journal.clone());
        let machine = FakeMac(Arc::clone(&fake));
        Self {
            folder,
            hashes,
            session: Session::new(Box::new(machine), models, shared_policy(), clock),
            events: Events::new(journal),
            fake,
            next_id: AtomicUsize::new(1),
        }
    }

    pub fn start_with(&self, args: &StartMeetingArgs) -> IpcResult<MeetingHandle> {
        let id = format!("m{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        self.session.start(args, id, Arc::new(self.events.clone()))
    }

    pub fn start(&self) -> IpcResult<MeetingHandle> {
        self.start_with(&start_args("speak", true))
    }

    pub fn stop(&self) -> IpcResult<MeetingHandle> {
        self.session.stop(&self.events)
    }

    /// How often one file of the model folder was hashed.
    pub fn hashed(&self, relative: &str) -> usize {
        let number = self.folder.number(relative);
        let hashed = self.hashes.hashed();
        hashed.iter().filter(|seen| **seen == number).count()
    }
}

pub fn start_args(mode: &str, headphones_confirmed: bool) -> StartMeetingArgs {
    StartMeetingArgs {
        mode: mode.to_string(),
        meeting_app_id: "meeting-app".to_string(),
        checks: StartChecks {
            headphones_confirmed,
            meeting_mic_confirmed: true,
        },
    }
}

/// State and detail of the model rows, in the order `sr`, `tr`, `enV`, `viV`.
pub fn model_rows(status: &SetupStatus) -> Vec<(SetupState, String)> {
    [
        SetupItemId::Sr,
        SetupItemId::Tr,
        SetupItemId::EnV,
        SetupItemId::ViV,
    ]
    .into_iter()
    .map(|id| {
        let item = status.items.iter().find(|item| item.id == id).unwrap();
        (item.state, item.detail.clone())
    })
    .collect()
}

pub fn model_states(status: &SetupStatus) -> Vec<SetupState> {
    model_rows(status)
        .into_iter()
        .map(|(state, _)| state)
        .collect()
}
