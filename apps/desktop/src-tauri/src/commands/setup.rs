//! `check_setup`: the real readiness probe of `audio-io`, mapped to the IPC
//! status (M1-41, FR-01).
//!
//! The probe opens the microphone, so it runs on a blocking thread and one at a
//! time. A permission seen as granted is kept for the app session; any other
//! result is probed again at the next call. Model states come from the model
//! manager (M1-04); until it reports, all four are `missing`.

use std::sync::{Arc, Mutex, PoisonError};

use audio_io::{
    probe_setup, DeviceListError, DeviceSnapshot, MicConfig, MicPermission, ModelState, ProbeEnv,
    ProbeGate, SetupItem as ReportItem, SetupItemId as ReportId, SetupReport,
    SetupState as ReportState, SystemEnv,
};

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

/// The state of one app session: whether the microphone permission is already
/// known to be granted, and the in-flight gate of the microphone probe. The
/// lock is held for a whole check, so checks run one at a time.
#[derive(Clone, Default)]
pub struct SetupCheck {
    granted: Arc<Mutex<bool>>,
    gate: ProbeGate,
}

impl SetupCheck {
    pub fn run(&self, env: &mut dyn ProbeEnv) -> SetupStatus {
        let mut granted = self.granted.lock().unwrap_or_else(PoisonError::into_inner);
        let outcome = probe_setup(env, MODELS_NOT_DOWNLOADED, *granted);
        if outcome.mic_permission == MicPermission::Granted {
            *granted = true;
        }
        status_from_report(&outcome.report)
    }

    /// The check on this Mac. Blocks for up to the sum of the probe limits.
    pub fn run_system(&self) -> SetupStatus {
        self.run(&mut SystemEnv::new(self.gate.clone()))
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
pub async fn check_setup(check: tauri::State<'_, SetupCheck>) -> IpcResult<SetupStatus> {
    let check = check.inner().clone();
    Ok(
        tauri::async_runtime::spawn_blocking(move || check.run_system())
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
        let first = check.run(&mut env);
        let second = check.run(&mut env);
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
            assert_ne!(mic_state(&check.run(&mut env)), SetupState::Ok, "{first:?}");
            assert_eq!(mic_state(&check.run(&mut env)), SetupState::Ok, "{first:?}");
            assert_eq!(env.probes.load(Ordering::SeqCst), 2, "{first:?}");
        }
    }

    #[test]
    fn overlapping_calls_run_one_at_a_time() {
        let check = SetupCheck::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let check = check.clone();
                let events = Arc::clone(&events);
                std::thread::spawn(move || {
                    let mut env = FakeEnv::new(&[MicPermission::Denied; 2]);
                    env.pause = Duration::from_millis(60);
                    env.events = events;
                    check.run(&mut env);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(
            *events.lock().unwrap(),
            ["start", "end", "start", "end"],
            "the second check started before the first ended"
        );
    }

    fn denied_status() -> SetupStatus {
        SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Denied]))
    }

    #[test]
    fn report_maps_to_status_items_in_order() {
        let status = SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Granted]));
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
        let status = SetupCheck::default().run(&mut FakeEnv::new(&[MicPermission::Granted]));
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
        let status = SetupCheck::default().run(&mut no_phones);
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
