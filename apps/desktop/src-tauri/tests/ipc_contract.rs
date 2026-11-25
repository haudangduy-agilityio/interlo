//! Each shared fixture in `fixtures/ipc/` deserializes into its Rust DTO and
//! serializes back to the same JSON value. The UI contract test reads the same
//! files, so a change to a shape breaks both sides.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use interlo_desktop::commands::meeting::{apply_mute, begin_meeting, end_meeting, MeetingState};
use interlo_desktop::commands::models::{request_cancel_download, request_download};
use interlo_desktop::commands::settings::validate_setting;
use interlo_desktop::commands::start_checks::{
    failed_checks, AudioFacts, OutputDevice, StartCheck,
};
use interlo_desktop::events::{
    BannerEvent, CaptionEvent, LatencyEvent, ModelProgressState, ModelsProgressEvent, StatusEvent,
    MODELS_PROGRESS,
};
use interlo_desktop::ipc::{
    AddNoteArgs, AppSettings, CancelModelDownloadArgs, DownloadModelArgs, ErrorCode, IpcError,
    MeetingHandle, MeetingMode, MeetingSummary, ModelInfo, Note, SetMuteArgs, SetSettingArgs,
    SetupGroup, SetupStatus, StartMeetingArgs,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/ipc")
        .join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Deserialize the fixture into `T`, serialize it back and compare the values.
fn round_trip<T: DeserializeOwned + Serialize>(name: &str) -> T {
    let expected = fixture(name);
    let dto: T = serde_json::from_value(expected.clone())
        .unwrap_or_else(|e| panic!("{name} does not fit its DTO: {e}"));
    let actual = serde_json::to_value(&dto).expect("serialize");
    assert_eq!(actual, expected, "{name} changed in the round trip");
    dto
}

#[test]
fn setup_status_round_trips() {
    let status: SetupStatus = round_trip("setup_status.json");
    assert_eq!(status.items.len(), 8);
    assert!(!status.can_start);
    assert_eq!(
        status.missing_groups,
        ["Microphone", "Meeting audio", "Models"]
    );
}

#[test]
fn setup_status_fixture_derived_fields_match_from_items() {
    let status: SetupStatus = round_trip("setup_status.json");
    let derived = SetupStatus::from_items(status.items.clone());
    assert_eq!(status.can_start, derived.can_start);
    assert_eq!(status.missing_groups, derived.missing_groups);
    assert_eq!(status.can_start, status.missing_groups.is_empty());
}

#[test]
fn model_ids_equal_the_setup_item_ids_of_the_models() {
    let models: Vec<ModelInfo> = round_trip("model_info_list.json");
    let status: SetupStatus = round_trip("setup_status.json");
    let setup_model_ids: Vec<String> = status
        .items
        .iter()
        .filter(|i| i.group == SetupGroup::Models)
        .map(|i| {
            serde_json::to_value(i.id)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let model_ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(model_ids, setup_model_ids);
}

#[test]
fn list_fixtures_are_newest_first() {
    let meetings: Vec<MeetingSummary> = round_trip("meeting_summary_list.json");
    assert!(meetings
        .windows(2)
        .all(|w| w[0].started_at_ms > w[1].started_at_ms));
    let notes: Vec<Note> = round_trip("note_list.json");
    assert!(notes
        .windows(2)
        .all(|w| w[0].created_at_ms > w[1].created_at_ms));
}

#[test]
fn ipc_error_setup_incomplete_round_trips() {
    let err: IpcError = round_trip("ipc_error_setup_incomplete.json");
    assert_eq!(err.code, ErrorCode::SetupIncomplete);
}

#[test]
fn model_info_list_round_trips() {
    let models: Vec<ModelInfo> = round_trip("model_info_list.json");
    assert_eq!(models.len(), 4);
    let tr = models.iter().find(|m| m.id == "tr").unwrap();
    assert!(tr.non_commercial);
    assert_eq!(
        tr.licence,
        "Apache-2.0. Training data is CC BY-NC-SA: personal use only"
    );
    assert!(models.iter().filter(|m| m.non_commercial).count() == 1);
    assert!(models.iter().all(|m| !m.licence.trim().is_empty()));
}

#[test]
fn model_info_requires_licence_and_non_commercial() {
    use serde_json::json;
    let base = json!({ "id": "sr", "name": "n", "sizeBytes": 1, "state": "ok" });
    assert!(serde_json::from_value::<ModelInfo>(base.clone()).is_err());
    let mut only_licence = base.clone();
    only_licence["licence"] = json!("See NOTICE");
    assert!(serde_json::from_value::<ModelInfo>(only_licence).is_err());
    let mut full = base;
    full["licence"] = json!("See NOTICE");
    full["nonCommercial"] = json!(false);
    assert!(serde_json::from_value::<ModelInfo>(full).is_ok());
}

#[test]
fn model_info_list_fixture_keeps_the_licence_fields() {
    // The shell's sample rows are compared with this fixture in the unit tests of
    // `commands::models`; a default build has no sample rows to compare.
    let fixture_models: Vec<ModelInfo> = round_trip("model_info_list.json");
    assert!(!fixture_models.is_empty());
    assert!(fixture_models.iter().all(|m| !m.licence.is_empty()));
}

#[test]
fn meeting_summary_list_round_trips() {
    let meetings: Vec<MeetingSummary> = round_trip("meeting_summary_list.json");
    assert_eq!(meetings.len(), 4);
}

#[test]
fn app_settings_round_trips() {
    let settings: AppSettings = round_trip("app_settings.json");
    assert_eq!(settings.auto_delete_days, None);
}

#[test]
fn note_round_trips() {
    let note: Note = round_trip("note.json");
    assert_eq!(note.tag.as_deref(), Some("work"));
}

#[test]
fn start_meeting_result_round_trips() {
    let handle: MeetingHandle = round_trip("start_meeting_result.json");
    assert_eq!(handle.meeting_id, "m-005");
}

#[test]
fn caption_event_round_trips() {
    let event: CaptionEvent = round_trip("events/caption.json");
    assert!(event.is_final);
}

#[test]
fn latency_event_round_trips() {
    let event: LatencyEvent = round_trip("events/latency.json");
    assert_eq!((event.p50_ms, event.p95_ms), (820, 1450));
}

#[test]
fn status_event_round_trips() {
    let _event: StatusEvent = round_trip("events/status.json");
}

#[test]
fn banner_event_round_trips() {
    let _event: BannerEvent = round_trip("events/banner.json");
}

#[test]
fn models_progress_event_round_trips() {
    let event: ModelsProgressEvent = round_trip("events/models_progress.json");
    assert_eq!(event.id, "tr");
    assert_eq!(event.state, ModelProgressState::Downloading);
    assert_eq!(
        (event.received_bytes, event.total_bytes),
        (123_456_789, 310_000_000)
    );
    assert_eq!(MODELS_PROGRESS, "models://progress");
}

#[test]
fn models_progress_states_are_the_four_wire_names() {
    use serde_json::json;
    for state in ["downloading", "verifying", "ok", "error"] {
        let value = json!({ "id": "sr", "receivedBytes": 0, "totalBytes": 1, "state": state });
        let event: ModelsProgressEvent = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&event).unwrap(), value);
    }
    let bad = json!({ "id": "sr", "receivedBytes": 0, "totalBytes": 1, "state": "missing" });
    assert!(serde_json::from_value::<ModelsProgressEvent>(bad).is_err());
}

#[test]
fn note_list_round_trips() {
    let notes: Vec<Note> = round_trip("note_list.json");
    assert_eq!(notes.len(), 2);
}

#[test]
fn every_error_code_has_a_fixture() {
    let cases = [
        (
            "ipc_error_setup_incomplete.json",
            ErrorCode::SetupIncomplete,
        ),
        ("ipc_error_not_implemented.json", ErrorCode::NotImplemented),
        ("ipc_error_meeting_running.json", ErrorCode::MeetingRunning),
        ("ipc_error_no_meeting.json", ErrorCode::NoMeeting),
        ("ipc_error_invalid_input.json", ErrorCode::InvalidInput),
        (
            "ipc_error_start_check_failed.json",
            ErrorCode::StartCheckFailed,
        ),
    ];
    for (name, code) in cases {
        let err: IpcError = round_trip(name);
        assert_eq!(err.code, code, "{name}");
    }
}

fn serialized<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn error_fixtures_equal_the_errors_the_commands_return() {
    let state = MeetingState::default();
    let status = SetupStatus::from_items(Vec::new());
    let no_meeting = end_meeting(&state).unwrap_err();
    assert_eq!(
        serialized(&no_meeting),
        fixture("ipc_error_no_meeting.json")
    );

    let ready: SetupStatus = serde_json::from_value(fixture("setup_status.json")).unwrap();
    let mut items = ready.items;
    for item in &mut items {
        item.state = interlo_desktop::ipc::SetupState::Ok;
    }
    let ready = SetupStatus::from_items(items);
    begin_meeting(&state, &ready, &[], MeetingMode::Both, "m-1".into()).unwrap();
    let running = begin_meeting(&state, &ready, &[], MeetingMode::Both, "m-2".into()).unwrap_err();
    assert_eq!(
        serialized(&running),
        fixture("ipc_error_meeting_running.json")
    );

    let blocked = begin_meeting(
        &MeetingState::default(),
        &status,
        &[],
        MeetingMode::Both,
        "m".into(),
    )
    .unwrap_err();
    assert_eq!(blocked.code, ErrorCode::SetupIncomplete);

    let unchecked = begin_meeting(
        &MeetingState::default(),
        &ready,
        &[StartCheck::Headphones, StartCheck::MeetingMic],
        MeetingMode::Both,
        "m".into(),
    )
    .unwrap_err();
    assert_eq!(
        serialized(&unchecked),
        fixture("ipc_error_start_check_failed.json")
    );

    let download = request_download("enV", true).unwrap_err();
    assert_eq!(
        serialized(&download),
        fixture("ipc_error_not_implemented.json")
    );

    let licence = request_download("enV", false).unwrap_err();
    assert_eq!(
        serialized(&licence),
        fixture("ipc_error_licence_not_accepted.json")
    );

    let invalid = validate_setting("autoDeleteDays", &serde_json::json!(0)).unwrap_err();
    assert_eq!(
        serialized(&invalid),
        fixture("ipc_error_invalid_input.json")
    );
}

#[test]
fn set_mute_args_fixture_fits_the_rust_arguments() {
    let args: SetMuteArgs = round_trip("args/set_mute.json");
    assert!(args.muted);
    assert_eq!(
        apply_mute(&MeetingState::default(), &args)
            .unwrap_err()
            .code,
        ErrorCode::NoMeeting
    );
}

/// Fixed facts for this test: headphones, capture and voice on separate devices.
struct HeadphonesOnSeparateDevices;

impl AudioFacts for HeadphonesOnSeparateDevices {
    fn output_device(&self) -> OutputDevice {
        OutputDevice::Headphones
    }

    fn capture_and_voice_share_device(&self) -> bool {
        false
    }
}

#[test]
fn start_meeting_args_fixture_fits_the_rust_arguments() {
    let args: StartMeetingArgs = round_trip("args/start_meeting.json");
    assert_eq!(args.mode, "both");
    assert_eq!(args.meeting_app_id, "sample-meeting-app");
    assert!(args.checks.headphones_confirmed && args.checks.meeting_mic_confirmed);
    assert!(failed_checks(&args, &HeadphonesOnSeparateDevices).is_empty());
}

#[test]
fn add_note_args_fixture_fits_the_rust_arguments() {
    let args: AddNoteArgs = round_trip("args/add_note.json");
    assert_eq!(args.tag.as_deref(), Some("work"));
}

#[test]
fn set_setting_args_fixture_fits_the_rust_arguments() {
    let args: SetSettingArgs = round_trip("args/set_setting.json");
    assert!(validate_setting(&args.key, &args.value).is_ok());
}

#[test]
fn download_model_args_fixture_fits_the_rust_arguments() {
    let args: DownloadModelArgs = round_trip("args/download_model.json");
    assert!(args.licence_accepted);
    assert_eq!(
        request_download(&args.id, args.licence_accepted)
            .unwrap_err()
            .code,
        ErrorCode::NotImplemented
    );
}

#[test]
fn cancel_model_download_args_fixture_fits_the_rust_arguments() {
    let args: CancelModelDownloadArgs = round_trip("args/cancel_model_download.json");
    assert_eq!(
        request_cancel_download(&args.id).unwrap_err().code,
        ErrorCode::NotImplemented
    );
    assert_eq!(
        request_cancel_download("nope").unwrap_err().code,
        ErrorCode::InvalidInput
    );
}

#[test]
fn a_renamed_or_extra_argument_does_not_fit_the_rust_arguments() {
    use serde_json::json;
    assert!(serde_json::from_value::<SetMuteArgs>(json!({ "dir": "out", "muted": true })).is_err());
    assert!(serde_json::from_value::<SetMuteArgs>(json!({ "direction": "out" })).is_err());
    let checks = json!({ "headphonesConfirmed": true, "meetingMicConfirmed": true });
    assert!(serde_json::from_value::<StartMeetingArgs>(
        json!({ "mode": "both", "meetingAppId": "a", "checks": checks, "x": 1 })
    )
    .is_err());
    assert!(serde_json::from_value::<StartMeetingArgs>(json!({ "mode": "both" })).is_err());
    assert!(serde_json::from_value::<StartMeetingArgs>(
        json!({ "mode": "both", "meetingApp": "a", "checks": checks })
    )
    .is_err());
    assert!(serde_json::from_value::<AddNoteArgs>(json!({ "body": "a", "tag": null })).is_err());
    assert!(serde_json::from_value::<SetSettingArgs>(json!({ "name": "a", "value": 1 })).is_err());
    assert!(serde_json::from_value::<DownloadModelArgs>(json!({ "model": "sr" })).is_err());
    assert!(serde_json::from_value::<DownloadModelArgs>(json!({ "id": "sr" })).is_err());
    assert!(serde_json::from_value::<DownloadModelArgs>(
        json!({ "id": "sr", "licenceAccepted": true, "url": "x" })
    )
    .is_err());
    assert!(
        serde_json::from_value::<CancelModelDownloadArgs>(json!({ "id": "sr", "x": 1 })).is_err()
    );
    assert!(serde_json::from_value::<CancelModelDownloadArgs>(json!({})).is_err());
}
