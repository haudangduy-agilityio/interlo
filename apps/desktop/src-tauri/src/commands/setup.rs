//! `check_setup`: the real readiness probe of `audio-io`, mapped to the IPC
//! status (M1-41, FR-01).
//!
//! The probe opens the microphone, so it runs on a blocking thread. Calls that
//! overlap share one run. While a meeting runs no microphone is opened. A
//! permission seen as granted is kept for the app session; any other result is
//! probed again at the next call. Model states come from the model manager
//! (M1-04); until it reports, all four are `missing`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use audio_io::{
    probe_setup, DeviceListError, DeviceSnapshot, MicConfig, MicPermission, ModelState, ProbeEnv,
    ProbeGate, SetupItem as ReportItem, SetupItemId as ReportId, SetupReport,
    SetupState as ReportState, SystemEnv,
};

#[cfg(not(feature = "demo-data"))]
use crate::commands::meeting::MeetingState;
use crate::ipc::{IpcResult, SetupGroup, SetupItem, SetupItemId, SetupState, SetupStatus};

/// No model manager reports yet (M1-04), so no model counts as downloaded.
const MODELS_NOT_DOWNLOADED: [ModelState; 4] = [ModelState::Missing; 4];

fn group_of(id: ReportId) -> SetupGroup {
    match id {
        ReportId::Mic | ReportId::Capture | ReportId::Phones => SetupGroup::Devices,
        ReportId::Vmic => SetupGroup::VirtualMicrophone,
        ReportId::Sr | ReportId::Tr | ReportId::EnV | ReportId::ViV => SetupGroup::Models,
    }
}

fn id_of(id: ReportId) -> SetupItemId {
    match id {
        ReportId::Mic => SetupItemId::Mic,
        ReportId::Capture => SetupItemId::Capture,
        ReportId::Vmic => SetupItemId::Vmic,
        ReportId::Phones => SetupItemId::Phones,
        ReportId::Sr => SetupItemId::Sr,
        ReportId::Tr => SetupItemId::Tr,
        ReportId::EnV => SetupItemId::EnV,
        ReportId::ViV => SetupItemId::ViV,
    }
}

fn state_of(state: ReportState) -> SetupState {
    match state {
        ReportState::Ok => SetupState::Ok,
        ReportState::Missing => SetupState::Missing,
        ReportState::Installing => SetupState::Installing,
        ReportState::Error => SetupState::Error,
    }
}

fn item_of(item: &ReportItem) -> SetupItem {
    SetupItem {
        id: id_of(item.id),
        group: group_of(item.id),
        state: state_of(item.state),
        detail: item.detail.clone(),
    }
}

/// The report as the webview sees it. `fix` and `warning` stay in Rust until the
/// commands and the Setup screen carry them (M1-13, M1-48).
pub fn status_from_report(report: &SetupReport) -> SetupStatus {
    SetupStatus::from_items(report.items.iter().map(item_of).collect())
}

/// The `mic` detail while a meeting runs and the permission is not known yet.
const MIC_WAITS_FOR_MEETING: &str = "Microphone check waits until the meeting ends";

/// One run of the check and the callers that wait for its answer.
struct Flight {
    answer: Mutex<Option<SetupStatus>>,
    ready: Condvar,
}

impl Flight {
    fn new() -> Self {
        Self {
            answer: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn wait(&self) -> SetupStatus {
        let mut answer = self.answer.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(status) = answer.as_ref() {
                return status.clone();
            }
            answer = self
                .ready
                .wait(answer)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Ends the run in flight when the leader returns, also when it panics: then
/// the waiting callers get the failed status instead of waiting for ever.
struct Leader<'a> {
    slot: &'a Mutex<Option<Arc<Flight>>>,
    flight: Arc<Flight>,
}

impl Leader<'_> {
    fn finish(&self, status: SetupStatus) {
        self.slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        *self
            .flight
            .answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(status);
        self.flight.ready.notify_all();
    }
}

impl Drop for Leader<'_> {
    fn drop(&mut self) {
        let finished = self
            .flight
            .answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some();
        if !finished {
            self.finish(failed_status());
        }
    }
}

/// A system that must not open the microphone: the probe answers `Unknown` and
/// records that it was asked. The other reads go to the real system.
struct NoMicrophoneOpen<'a> {
    inner: &'a mut dyn ProbeEnv,
    asked: bool,
}

impl ProbeEnv for NoMicrophoneOpen<'_> {
    fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
        self.inner.read_devices()
    }
    fn probe_microphone(&mut self, _config: &MicConfig) -> MicPermission {
        self.asked = true;
        MicPermission::Unknown
    }
    fn tap_available(&mut self) -> bool {
        self.inner.tap_available()
    }
}

/// The state of one app session: whether the microphone permission is already
/// known to be granted, the in-flight gate of the microphone probe and the run
/// in flight. Calls that overlap share one run: the first call runs the check,
/// the others wait for it and get the same answer.
#[derive(Clone, Default)]
pub struct SetupCheck {
    granted: Arc<AtomicBool>,
    flight: Arc<Mutex<Option<Arc<Flight>>>>,
    gate: ProbeGate,
}

impl SetupCheck {
    /// The check. While a meeting runs the microphone is not opened.
    pub fn run(&self, env: &mut dyn ProbeEnv, meeting_running: bool) -> SetupStatus {
        let (flight, leads) = {
            let mut slot = self.flight.lock().unwrap_or_else(PoisonError::into_inner);
            match slot.as_ref() {
                Some(running) => (Arc::clone(running), false),
                None => {
                    let flight = Arc::new(Flight::new());
                    *slot = Some(Arc::clone(&flight));
                    (flight, true)
                }
            }
        };
        if !leads {
            return flight.wait();
        }
        let leader = Leader {
            slot: &self.flight,
            flight,
        };
        let status = self.run_once(env, meeting_running);
        leader.finish(status.clone());
        status
    }

    fn run_once(&self, env: &mut dyn ProbeEnv, meeting_running: bool) -> SetupStatus {
        let known = self.granted.load(Ordering::SeqCst);
        if meeting_running && !known {
            let mut closed = NoMicrophoneOpen {
                inner: env,
                asked: false,
            };
            let outcome = probe_setup(&mut closed, MODELS_NOT_DOWNLOADED, false);
            let mut status = status_from_report(&outcome.report);
            if closed.asked {
                if let Some(mic) = status.items.iter_mut().find(|i| i.id == SetupItemId::Mic) {
                    mic.detail = MIC_WAITS_FOR_MEETING.to_string();
                }
            }
            return status;
        }
        let outcome = probe_setup(env, MODELS_NOT_DOWNLOADED, known);
        if outcome.mic_permission == MicPermission::Granted {
            self.granted.store(true, Ordering::SeqCst);
        }
        status_from_report(&outcome.report)
    }

    /// The check on this Mac. Blocks for up to the sum of the probe limits.
    pub fn run_system(&self, meeting_running: bool) -> SetupStatus {
        self.run(&mut SystemEnv::new(self.gate.clone()), meeting_running)
    }
}

/// The answer when the blocking task cannot finish: the four device rows say so.
pub fn failed_status() -> SetupStatus {
    struct Failed;
    impl ProbeEnv for Failed {
        fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
            Err(DeviceListError::ToolFailed)
        }
        fn probe_microphone(&mut self, _config: &MicConfig) -> MicPermission {
            MicPermission::Unknown
        }
        fn tap_available(&mut self) -> bool {
            false
        }
    }
    status_from_report(&probe_setup(&mut Failed, MODELS_NOT_DOWNLOADED, false).report)
}

#[cfg(any(test, feature = "demo-data"))]
fn item(id: SetupItemId, group: SetupGroup, state: SetupState, detail: &str) -> SetupItem {
    SetupItem {
        id,
        group,
        state,
        detail: detail.to_string(),
    }
}

/// Fixed sample: everything ready except the English voice model. Tests and a
/// `demo-data` build only: a default build never reports sample setup data.
#[cfg(any(test, feature = "demo-data"))]
pub fn sample_setup_status() -> SetupStatus {
    use SetupGroup::{Devices, Models, VirtualMicrophone};
    use SetupItemId::*;
    use SetupState::{Missing, Ok};
    SetupStatus::from_items(vec![
        item(Mic, Devices, Ok, "Built-in microphone"),
        item(Capture, Devices, Ok, "2-channel virtual device"),
        item(Vmic, VirtualMicrophone, Ok, "16-channel virtual device"),
        item(Phones, Devices, Ok, "Headphones connected"),
        item(Sr, Models, Ok, "Speech recognition model"),
        item(Tr, Models, Ok, "Translation model"),
        item(EnV, Models, Missing, "English voice model not downloaded"),
        item(ViV, Models, Ok, "Vietnamese voice model"),
    ])
}

#[cfg(not(feature = "demo-data"))]
#[tauri::command]
pub async fn check_setup(
    check: tauri::State<'_, SetupCheck>,
    meeting: tauri::State<'_, MeetingState>,
) -> IpcResult<SetupStatus> {
    let check = check.inner().clone();
    let running = meeting.is_running();
    Ok(
        tauri::async_runtime::spawn_blocking(move || check.run_system(running))
            .await
            .unwrap_or_else(|_| failed_status()),
    )
}

/// The sample status of a `demo-data` build.
#[cfg(any(test, feature = "demo-data"))]
pub fn current_setup_status() -> IpcResult<SetupStatus> {
    Ok(sample_setup_status())
}

/// A `demo-data` build answers the fixed sample.
#[cfg(feature = "demo-data")]
#[tauri::command]
pub async fn check_setup() -> IpcResult<SetupStatus> {
    current_setup_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_status_uses_camel_case_names() {
        let json = serde_json::to_value(sample_setup_status()).unwrap();
        assert_eq!(json["canStart"], false);
        assert_eq!(json["missingGroups"], serde_json::json!(["Models"]));
        assert!(json.get("can_start").is_none());
        let first = &json["items"][0];
        assert_eq!(first["id"], "mic");
        assert_eq!(first["group"], "devices");
        assert_eq!(first["state"], "ok");
        let ids: Vec<&str> = json["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            ["mic", "capture", "vmic", "phones", "sr", "tr", "enV", "viV"]
        );
        assert_eq!(json["items"][2]["group"], "virtualMicrophone");
    }

    #[test]
    fn can_start_when_all_required_items_are_ok() {
        let mut status = sample_setup_status();
        for item in &mut status.items {
            item.state = SetupState::Ok;
        }
        let status = SetupStatus::from_items(status.items);
        assert!(status.can_start);
        assert!(status.missing_groups.is_empty());
    }

    #[test]
    fn missing_virtual_microphone_is_named_with_its_display_name() {
        let mut items = sample_setup_status().items;
        for item in &mut items {
            item.state = SetupState::Ok;
        }
        items[2].state = SetupState::Missing;
        let status = SetupStatus::from_items(items);
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Virtual microphone"]);
    }

    #[test]
    fn missing_microphone_or_meeting_audio_blocks_start_with_display_names() {
        let ready = || {
            let mut items = sample_setup_status().items;
            for item in &mut items {
                item.state = SetupState::Ok;
            }
            items
        };
        let mut items = ready();
        items[0].state = SetupState::Missing;
        let status = SetupStatus::from_items(items);
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Microphone"]);

        let mut items = ready();
        items[1].state = SetupState::Error;
        let status = SetupStatus::from_items(items);
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Meeting audio"]);
    }

    #[test]
    fn missing_headphones_do_not_block_start() {
        let mut items = sample_setup_status().items;
        for item in &mut items {
            item.state = SetupState::Ok;
        }
        items[3].state = SetupState::Missing;
        let status = SetupStatus::from_items(items);
        assert!(status.can_start);
        assert!(status.missing_groups.is_empty());
    }

    // ---- the real check, with a fake system (M1-41) ----

    use audio_io::{DeviceInfo, DeviceKind, Transport};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

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

    fn devices() -> DeviceSnapshot {
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

    /// A system whose microphone answers come from a list, one per probe.
    struct FakeEnv {
        devices: Result<DeviceSnapshot, DeviceListError>,
        answers: Vec<MicPermission>,
        probes: Arc<AtomicUsize>,
        pause: Duration,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl FakeEnv {
        fn new(answers: &[MicPermission]) -> Self {
            Self {
                devices: Ok(devices()),
                answers: answers.to_vec(),
                probes: Arc::new(AtomicUsize::new(0)),
                pause: Duration::ZERO,
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl ProbeEnv for FakeEnv {
        fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
            self.devices.clone()
        }
        fn probe_microphone(&mut self, _config: &MicConfig) -> MicPermission {
            let n = self.probes.fetch_add(1, Ordering::SeqCst);
            self.events.lock().unwrap().push("start");
            std::thread::sleep(self.pause);
            self.events.lock().unwrap().push("end");
            self.answers
                .get(n)
                .copied()
                .unwrap_or(MicPermission::Unknown)
        }
        fn tap_available(&mut self) -> bool {
            false
        }
    }

    fn mic_state(status: &SetupStatus) -> SetupState {
        status.items[0].state
    }

    #[test]
    fn granted_is_kept_and_not_probed_again() {
        let check = SetupCheck::default();
        let mut env = FakeEnv::new(&[MicPermission::Granted, MicPermission::Denied]);
        let first = check.run(&mut env, false);
        let second = check.run(&mut env, false);
        assert_eq!(mic_state(&first), SetupState::Ok);
        assert_eq!(second.items[0], first.items[0]);
        assert_eq!(env.probes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn other_results_are_probed_again() {
        for first in [
            MicPermission::Denied,
            MicPermission::NoSignal,
            MicPermission::NotDetermined,
            MicPermission::Unknown,
        ] {
            let check = SetupCheck::default();
            let mut env = FakeEnv::new(&[first, MicPermission::Granted]);
            assert_ne!(
                mic_state(&check.run(&mut env, false)),
                SetupState::Ok,
                "{first:?}"
            );
            assert_eq!(
                mic_state(&check.run(&mut env, false)),
                SetupState::Ok,
                "{first:?}"
            );
            assert_eq!(env.probes.load(Ordering::SeqCst), 2, "{first:?}");
        }
    }

    /// A system whose microphone probe ends in a panic after a pause.
    struct PanicEnv;

    impl ProbeEnv for PanicEnv {
        fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
            Ok(devices())
        }
        fn probe_microphone(&mut self, _config: &MicConfig) -> MicPermission {
            std::thread::sleep(Duration::from_millis(120));
            panic!("the probe failed on purpose");
        }
        fn tap_available(&mut self) -> bool {
            false
        }
    }

    #[test]
    fn overlapping_calls_share_one_run() {
        let check = SetupCheck::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let probes = Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..3u64)
            .map(|n| {
                let check = check.clone();
                let events = Arc::clone(&events);
                let probes = Arc::clone(&probes);
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(40 * n));
                    let mut env = FakeEnv::new(&[MicPermission::Denied; 3]);
                    env.pause = Duration::from_millis(200);
                    env.probes = probes;
                    env.events = Arc::clone(&events);
                    let status = check.run(&mut env, false);
                    events.lock().unwrap().push("returned");
                    status
                })
            })
            .collect();
        let answers: Vec<SetupStatus> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(
            probes.load(Ordering::SeqCst),
            1,
            "one run opens the microphone once"
        );
        assert_eq!(answers[1], answers[0]);
        assert_eq!(answers[2], answers[0]);
        assert_eq!(
            *events.lock().unwrap(),
            ["start", "end", "returned", "returned", "returned"],
            "a waiting caller returned before the run ended"
        );
        // The counter is shared, so this second run reads answer 1.
        let mut later = FakeEnv::new(&[MicPermission::Denied, MicPermission::Granted]);
        later.probes = Arc::clone(&probes);
        assert_eq!(mic_state(&check.run(&mut later, false)), SetupState::Ok);
        assert_eq!(
            probes.load(Ordering::SeqCst),
            2,
            "a call after the run starts a new run"
        );
    }

    #[test]
    fn a_panic_in_the_run_gives_the_failed_status_to_the_waiter() {
        let check = SetupCheck::default();
        let leader = {
            let check = check.clone();
            std::thread::spawn(move || check.run(&mut PanicEnv, false))
        };
        std::thread::sleep(Duration::from_millis(40));
        let waiter = {
            let check = check.clone();
            std::thread::spawn(move || {
                check.run(&mut FakeEnv::new(&[MicPermission::Granted]), false)
            })
        };
        assert!(leader.join().is_err(), "the leader panics");
        assert_eq!(waiter.join().unwrap(), failed_status());
        let mut env = FakeEnv::new(&[MicPermission::Granted]);
        assert_eq!(mic_state(&check.run(&mut env, false)), SetupState::Ok);
        assert_eq!(env.probes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_running_meeting_opens_no_microphone() {
        let check = SetupCheck::default();
        let mut env = FakeEnv::new(&[MicPermission::Granted]);
        let status = check.run(&mut env, true);
        assert_eq!(env.probes.load(Ordering::SeqCst), 0);
        assert_eq!(status.items[0].state, SetupState::Error);
        assert_eq!(
            status.items[0].detail,
            "Microphone check waits until the meeting ends"
        );
        assert_eq!(mic_state(&check.run(&mut env, false)), SetupState::Ok);
        assert_eq!(env.probes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_running_meeting_keeps_a_known_grant() {
        let check = SetupCheck::default();
        let mut env = FakeEnv::new(&[MicPermission::Granted]);
        let before = check.run(&mut env, false);
        let during = check.run(&mut env, true);
        assert_eq!(during.items[0], before.items[0]);
        assert_eq!(mic_state(&during), SetupState::Ok);
        assert_eq!(env.probes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_running_meeting_leaves_the_other_rows_alone() {
        let quiet = SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Denied]), false);
        let busy = SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Denied]), true);
        assert_eq!(busy.items[1..], quiet.items[1..]);
    }

    #[test]
    fn a_running_meeting_with_no_microphone_keeps_that_row() {
        let mut env = FakeEnv::new(&[MicPermission::Granted]);
        env.devices = Ok(DeviceSnapshot::default());
        let quiet = SetupCheck::default().run(&mut env, false);
        let busy = SetupCheck::default().run(&mut env, true);
        assert_eq!(quiet.items[0].state, SetupState::Missing);
        assert_eq!(busy.items[0], quiet.items[0]);
    }

    #[test]
    fn the_command_passes_the_meeting_flag() {
        let src = include_str!("setup.rs");
        // Built from two parts so that this test does not find itself.
        let signature = format!("{}{}", "pub async fn check_", "setup(\n");
        let start = src
            .find(&signature)
            .expect("the default-build command exists");
        let body = &src[start..start + src[start..].find("\n}\n").unwrap()];
        assert!(body.contains("meeting.is_running()"), "{body}");
        assert!(body.contains("run_system(running)"), "{body}");
    }

    fn denied_status() -> SetupStatus {
        SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Denied]), false)
    }

    #[test]
    fn report_maps_to_status_items_in_order() {
        let status = SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Granted]), false);
        let got: Vec<(SetupItemId, SetupGroup, SetupState)> = status
            .items
            .iter()
            .map(|i| (i.id, i.group, i.state))
            .collect();
        assert_eq!(
            got,
            [
                (SetupItemId::Mic, SetupGroup::Devices, SetupState::Ok),
                (SetupItemId::Capture, SetupGroup::Devices, SetupState::Ok),
                (
                    SetupItemId::Vmic,
                    SetupGroup::VirtualMicrophone,
                    SetupState::Ok
                ),
                (SetupItemId::Phones, SetupGroup::Devices, SetupState::Ok),
                (SetupItemId::Sr, SetupGroup::Models, SetupState::Missing),
                (SetupItemId::Tr, SetupGroup::Models, SetupState::Missing),
                (SetupItemId::EnV, SetupGroup::Models, SetupState::Missing),
                (SetupItemId::ViV, SetupGroup::Models, SetupState::Missing),
            ]
        );
        assert_eq!(status.items[0].detail, "Mic");
        assert_eq!(status.items[1].detail, "2-channel virtual device");
        assert_eq!(
            status.items[4].detail,
            "Speech recognition model not downloaded"
        );
    }

    #[test]
    fn every_report_state_maps_to_its_status_state() {
        let item = |id, state| ReportItem {
            id,
            state,
            detail: String::new(),
            fix: None,
            warning: None,
        };
        let report = SetupReport {
            items: vec![
                item(ReportId::Mic, ReportState::Ok),
                item(ReportId::Capture, ReportState::Missing),
                item(ReportId::Vmic, ReportState::Installing),
                item(ReportId::Phones, ReportState::Error),
            ],
        };
        let states: Vec<SetupState> = status_from_report(&report)
            .items
            .iter()
            .map(|i| i.state)
            .collect();
        assert_eq!(
            states,
            [
                SetupState::Ok,
                SetupState::Missing,
                SetupState::Installing,
                SetupState::Error
            ]
        );
    }

    #[test]
    fn models_missing_lists_the_models_group_and_headphones_add_nothing() {
        let status = SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Granted]), false);
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Models"]);
        let mut no_phones = FakeEnv::new(&[MicPermission::Granted]);
        no_phones.devices = Ok(DeviceSnapshot {
            devices: devices()
                .devices
                .into_iter()
                .filter(|d| d.kind != DeviceKind::Headphones)
                .collect(),
            skipped: 0,
        });
        let status = SetupCheck::default().run(&mut no_phones, false);
        assert_eq!(status.items[3].state, SetupState::Missing);
        assert_eq!(status.missing_groups, ["Models"]);
    }

    #[test]
    fn denied_mic_adds_the_microphone_group() {
        let status = denied_status();
        assert_eq!(mic_state(&status), SetupState::Missing);
        assert_eq!(status.missing_groups, ["Microphone", "Models"]);
        assert!(!status.can_start);
    }

    #[test]
    fn status_json_keys_equal_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/ipc/setup_status.json"
        )))
        .unwrap();
        let keys = |v: &serde_json::Value| -> Vec<String> {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        let json = serde_json::to_value(denied_status()).unwrap();
        assert_eq!(keys(&json), keys(&fixture));
        for item in json["items"].as_array().unwrap() {
            assert_eq!(keys(item), keys(&fixture["items"][0]));
        }
    }

    #[test]
    fn check_returns_eight_rows_in_order_with_models_missing() {
        let status = denied_status();
        let ids: Vec<SetupItemId> = status.items.iter().map(|i| i.id).collect();
        assert_eq!(
            ids,
            [
                SetupItemId::Mic,
                SetupItemId::Capture,
                SetupItemId::Vmic,
                SetupItemId::Phones,
                SetupItemId::Sr,
                SetupItemId::Tr,
                SetupItemId::EnV,
                SetupItemId::ViV
            ]
        );
        assert!(status.items[4..]
            .iter()
            .all(|i| i.state == SetupState::Missing));
    }

    #[test]
    fn a_failed_blocking_task_gives_the_failed_device_status() {
        let status = failed_status();
        assert_eq!(status.items.len(), 8);
        for item in &status.items[..4] {
            assert_eq!(item.state, SetupState::Error, "{:?}", item.id);
            assert_eq!(item.detail, "Devices cannot be read");
        }
        assert!(!status.can_start);
        assert_eq!(
            status.missing_groups,
            [
                "Microphone",
                "Meeting audio",
                "Virtual microphone",
                "Models"
            ]
        );
    }

    #[cfg(feature = "demo-data")]
    #[test]
    fn demo_build_returns_the_sample() {
        let sample = sample_setup_status();
        assert_eq!(sample.items[6].state, SetupState::Missing);
        assert_eq!(sample.missing_groups, ["Models"]);
    }
}
