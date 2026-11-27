//! DTOs and the error type shared by every command.
//!
//! Field names are camelCase on the wire; enum values are lowercase or
//! SCREAMING_SNAKE_CASE as noted per type.

use serde::{Deserialize, Serialize};

/// Stable error codes the UI can switch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    SetupIncomplete,
    StartCheckFailed,
    NotImplemented,
    MeetingRunning,
    NoMeeting,
    InvalidInput,
    /// An accepted start could not load the speech engines or open the microphone.
    StartFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
    /// Failed check ids. Only `START_CHECK_FAILED` sets it; other errors omit it from the JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<String>>,
}

impl IpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// A command needs a meeting and there is none.
    pub fn no_meeting() -> Self {
        Self::new(ErrorCode::NoMeeting, "No meeting is running.")
    }

    /// A start while a meeting exists.
    pub fn meeting_running() -> Self {
        Self::new(ErrorCode::MeetingRunning, "A meeting is already running.")
    }

    pub fn start_check_failed(message: impl Into<String>, details: Vec<String>) -> Self {
        Self {
            details: Some(details),
            ..Self::new(ErrorCode::StartCheckFailed, message)
        }
    }

    pub fn not_implemented(what: &str) -> Self {
        Self::new(
            ErrorCode::NotImplemented,
            format!("{what} is not available yet."),
        )
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }
}

pub type IpcResult<T> = Result<T, IpcError>;

// ---- setup ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetupItemId {
    #[serde(rename = "mic")]
    Mic,
    #[serde(rename = "capture")]
    Capture,
    #[serde(rename = "vmic")]
    Vmic,
    #[serde(rename = "phones")]
    Phones,
    #[serde(rename = "sr")]
    Sr,
    #[serde(rename = "tr")]
    Tr,
    #[serde(rename = "enV")]
    EnV,
    #[serde(rename = "viV")]
    ViV,
}

/// Items that must be `ok` before a meeting can start, in the order their
/// display names appear in `missingGroups`. One rule for every mode.
pub const REQUIRED_SETUP_ITEMS: [SetupItemId; 7] = [
    SetupItemId::Mic,
    SetupItemId::Capture,
    SetupItemId::Vmic,
    SetupItemId::Sr,
    SetupItemId::Tr,
    SetupItemId::EnV,
    SetupItemId::ViV,
];

impl SetupItemId {
    /// Items that must be `ok` before a meeting can start.
    pub fn is_required(self) -> bool {
        REQUIRED_SETUP_ITEMS.contains(&self)
    }

    /// The four models. Their ids are also the `ModelInfo.id` values.
    pub fn is_model(self) -> bool {
        matches!(self, Self::Sr | Self::Tr | Self::EnV | Self::ViV)
    }

    /// Name shown to the user in `missingGroups` when this required item is not
    /// ready. All four models share one name. `None` for an item that never blocks.
    pub fn missing_name(self) -> Option<&'static str> {
        match self {
            Self::Mic => Some("Microphone"),
            Self::Capture => Some("Meeting audio"),
            Self::Vmic => Some("Virtual microphone"),
            Self::Sr | Self::Tr | Self::EnV | Self::ViV => Some("Models"),
            Self::Phones => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupGroup {
    Devices,
    VirtualMicrophone,
    Models,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SetupState {
    Ok,
    Missing,
    Installing,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupItem {
    pub id: SetupItemId,
    pub group: SetupGroup,
    pub state: SetupState,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub items: Vec<SetupItem>,
    pub can_start: bool,
    pub missing_groups: Vec<String>,
}

impl SetupStatus {
    /// Derive `canStart` and `missingGroups` from the item list. A required item
    /// is ready only when it is listed and every entry with its id is `ok`, so an
    /// absent item blocks a start like a missing one.
    pub fn from_items(items: Vec<SetupItem>) -> Self {
        let mut missing_groups: Vec<String> = Vec::new();
        for id in REQUIRED_SETUP_ITEMS {
            let mut entries = items.iter().filter(|i| i.id == id).peekable();
            let ready = entries.peek().is_some() && entries.all(|i| i.state == SetupState::Ok);
            if ready {
                continue;
            }
            if let Some(name) = id.missing_name() {
                if !missing_groups.iter().any(|n| n == name) {
                    missing_groups.push(name.to_string());
                }
            }
        }
        Self {
            items,
            can_start: missing_groups.is_empty(),
            missing_groups,
        }
    }
}

// ---- command arguments ----
//
// One struct per command that takes arguments. Field names are the wire names
// (camelCase). Each command builds its struct from its own parameters, so a
// renamed parameter fails to compile and a renamed field fails the contract test.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartMeetingArgs {
    /// Parsed by `commands::meeting::parse_mode`, so a bad value is `INVALID_INPUT`.
    pub mode: String,
    /// Identifier of the chosen meeting app. Blank fails the `meetingApp` check.
    pub meeting_app_id: String,
    pub checks: StartChecks,
}

/// What the owner confirmed at this Start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartChecks {
    pub headphones_confirmed: bool,
    pub meeting_mic_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetMuteArgs {
    pub direction: Direction,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddNoteArgs {
    pub text: String,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetSettingArgs {
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadModelArgs {
    pub id: String,
    /// The owner ticked the licence box in the dialog. False refuses the download.
    pub licence_accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelModelDownloadArgs {
    pub id: String,
}

// ---- models ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelState {
    Ok,
    Missing,
    Downloading,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub state: ModelState,
    /// Short licence line shown in the download dialog.
    pub licence: String,
    /// True when the licence forbids commercial use.
    pub non_commercial: bool,
}

// ---- meeting ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingMode {
    Off,
    Listen,
    Speak,
    Both,
}

/// The wire form of `playback::Direction`, the one definition of the concept:
/// `In` is the partner's line (`Incoming`) and `Out` is the owner's (`Outgoing`).
/// The names `"in"` and `"out"` are part of the IPC contract; the conversions
/// below keep the two types from drifting apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Out,
    In,
}

impl From<playback::Direction> for Direction {
    fn from(d: playback::Direction) -> Self {
        match d {
            playback::Direction::Incoming => Self::In,
            playback::Direction::Outgoing => Self::Out,
        }
    }
}

impl From<Direction> for playback::Direction {
    fn from(d: Direction) -> Self {
        match d {
            Direction::In => Self::Incoming,
            Direction::Out => Self::Outgoing,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingHandle {
    pub meeting_id: String,
}

// ---- history ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingStatus {
    Ready,
    Generating,
    Interrupted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub started_at_ms: u64,
    pub duration_sec: u64,
    pub status: MeetingStatus,
}

// ---- settings ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiLanguage {
    En,
    Vi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub ui_language: UiLanguage,
    pub ducking: bool,
    pub auto_delete_days: Option<u32>,
}

// ---- notes ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub text: String,
    pub tag: Option<String>,
    pub created_at_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_is_screaming_snake_case() {
        let err = IpcError::new(ErrorCode::SetupIncomplete, "x");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "SETUP_INCOMPLETE");
        assert_eq!(json["message"], "x");
        for (code, name) in [
            (ErrorCode::NotImplemented, "NOT_IMPLEMENTED"),
            (ErrorCode::MeetingRunning, "MEETING_RUNNING"),
            (ErrorCode::NoMeeting, "NO_MEETING"),
            (ErrorCode::InvalidInput, "INVALID_INPUT"),
            (ErrorCode::StartCheckFailed, "START_CHECK_FAILED"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), name);
        }
    }

    #[test]
    fn details_are_skipped_in_json_when_absent_and_kept_when_present() {
        let plain = serde_json::to_value(IpcError::invalid_input("x")).unwrap();
        assert!(plain.get("details").is_none());
        let with = IpcError::start_check_failed("m", vec!["headphones".to_string()]);
        assert_eq!(with.code, ErrorCode::StartCheckFailed);
        assert_eq!(
            serde_json::to_value(&with).unwrap(),
            serde_json::json!({
                "code": "START_CHECK_FAILED",
                "message": "m",
                "details": ["headphones"]
            })
        );
        let back: IpcError =
            serde_json::from_value(serde_json::json!({ "code": "NO_MEETING", "message": "m" }))
                .unwrap();
        assert_eq!(back.details, None);
    }

    #[test]
    fn start_meeting_args_use_camel_case_and_reject_unknown_keys() {
        let json = serde_json::json!({
            "mode": "both",
            "meetingAppId": "app",
            "checks": { "headphonesConfirmed": true, "meetingMicConfirmed": false }
        });
        let args: StartMeetingArgs = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(args.meeting_app_id, "app");
        assert!(args.checks.headphones_confirmed);
        assert!(!args.checks.meeting_mic_confirmed);
        assert_eq!(serde_json::to_value(&args).unwrap(), json);

        for bad in [
            serde_json::json!({ "mode": "both" }),
            serde_json::json!({ "mode": "both", "meetingAppId": "a" }),
            serde_json::json!({ "mode": "both", "meetingAppId": "a",
                "checks": { "headphonesConfirmed": true } }),
            serde_json::json!({ "mode": "both", "meetingAppId": "a",
                "checks": { "headphonesConfirmed": true, "meetingMicConfirmed": true, "x": 1 } }),
            serde_json::json!({ "mode": "both", "meetingAppId": "a", "x": 1,
                "checks": { "headphonesConfirmed": true, "meetingMicConfirmed": true } }),
        ] {
            assert!(
                serde_json::from_value::<StartMeetingArgs>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn direction_in_serializes_as_in() {
        assert_eq!(serde_json::to_value(Direction::In).unwrap(), "in");
        assert_eq!(serde_json::to_value(Direction::Out).unwrap(), "out");
    }

    #[test]
    fn direction_converts_to_and_from_the_playback_type() {
        use playback::Direction as Domain;
        for (wire, domain) in [
            (Direction::In, Domain::Incoming),
            (Direction::Out, Domain::Outgoing),
        ] {
            assert_eq!(Direction::from(domain), wire);
            assert_eq!(Domain::from(wire), domain);
        }
    }

    #[test]
    fn capture_and_vmic_are_the_wire_names() {
        assert_eq!(
            serde_json::to_value(SetupItemId::Capture).unwrap(),
            "capture"
        );
        assert_eq!(serde_json::to_value(SetupItemId::Vmic).unwrap(), "vmic");
        for old in ["bh2", "bh16"] {
            assert!(serde_json::from_value::<SetupItemId>(serde_json::json!(old)).is_err());
        }
    }

    fn status_with(states: &[(SetupItemId, SetupState)]) -> SetupStatus {
        let items = states
            .iter()
            .map(|(id, state)| SetupItem {
                id: *id,
                group: SetupGroup::Devices,
                state: *state,
                detail: String::new(),
            })
            .collect();
        SetupStatus::from_items(items)
    }

    fn all_required_ok_except(id: SetupItemId, state: SetupState) -> SetupStatus {
        let mut states: Vec<(SetupItemId, SetupState)> = REQUIRED_SETUP_ITEMS
            .iter()
            .map(|i| (*i, SetupState::Ok))
            .collect();
        states.push((SetupItemId::Phones, SetupState::Ok));
        for entry in &mut states {
            if entry.0 == id {
                entry.1 = state;
            }
        }
        status_with(&states)
    }

    #[test]
    fn required_items_are_mic_capture_vmic_and_the_four_models() {
        assert_eq!(
            REQUIRED_SETUP_ITEMS,
            [
                SetupItemId::Mic,
                SetupItemId::Capture,
                SetupItemId::Vmic,
                SetupItemId::Sr,
                SetupItemId::Tr,
                SetupItemId::EnV,
                SetupItemId::ViV,
            ]
        );
        for id in REQUIRED_SETUP_ITEMS {
            assert!(id.is_required(), "{id:?}");
        }
        assert!(!SetupItemId::Phones.is_required());
    }

    #[test]
    fn each_required_item_alone_blocks_start_with_its_display_name() {
        for (id, name) in [
            (SetupItemId::Mic, "Microphone"),
            (SetupItemId::Capture, "Meeting audio"),
            (SetupItemId::Vmic, "Virtual microphone"),
            (SetupItemId::Sr, "Models"),
            (SetupItemId::Tr, "Models"),
            (SetupItemId::EnV, "Models"),
            (SetupItemId::ViV, "Models"),
        ] {
            for state in [
                SetupState::Missing,
                SetupState::Installing,
                SetupState::Error,
            ] {
                let status = all_required_ok_except(id, state);
                assert!(!status.can_start, "{id:?} {state:?}");
                assert_eq!(status.missing_groups, [name], "{id:?} {state:?}");
            }
        }
    }

    #[test]
    fn headphones_never_block_start() {
        let status = all_required_ok_except(SetupItemId::Phones, SetupState::Missing);
        assert!(status.can_start);
        assert!(status.missing_groups.is_empty());
    }

    #[test]
    fn missing_groups_follow_the_fixed_order_and_name_models_once() {
        let status = status_with(&[
            (SetupItemId::ViV, SetupState::Missing),
            (SetupItemId::Vmic, SetupState::Missing),
            (SetupItemId::Sr, SetupState::Error),
            (SetupItemId::Capture, SetupState::Missing),
            (SetupItemId::Mic, SetupState::Missing),
            (SetupItemId::Tr, SetupState::Ok),
            (SetupItemId::EnV, SetupState::Ok),
        ]);
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

    #[test]
    fn absent_required_items_count_as_missing() {
        let empty = SetupStatus::from_items(Vec::new());
        assert!(!empty.can_start);
        assert_eq!(
            empty.missing_groups,
            [
                "Microphone",
                "Meeting audio",
                "Virtual microphone",
                "Models"
            ]
        );

        let mut states: Vec<(SetupItemId, SetupState)> = REQUIRED_SETUP_ITEMS
            .iter()
            .filter(|id| **id != SetupItemId::Capture)
            .map(|id| (*id, SetupState::Ok))
            .collect();
        let no_capture = status_with(&states);
        assert!(!no_capture.can_start);
        assert_eq!(no_capture.missing_groups, ["Meeting audio"]);

        states.retain(|(id, _)| *id != SetupItemId::EnV);
        let no_capture_no_voice = status_with(&states);
        assert_eq!(
            no_capture_no_voice.missing_groups,
            ["Meeting audio", "Models"]
        );
    }
}
