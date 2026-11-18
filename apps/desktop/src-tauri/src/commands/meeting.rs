use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, State};

use crate::clock::{now_ms, unique_id};
#[cfg(any(test, feature = "demo-data"))]
use crate::commands::setup::sample_setup_status;
#[cfg(any(test, feature = "demo-data"))]
use crate::commands::start_checks::SampleAudioFacts;
use crate::commands::start_checks::{failed_checks, refusal, AudioFacts, StartCheck};
use crate::events::{RunState, StatusSink};
use crate::ipc::{
    Direction, ErrorCode, IpcError, IpcResult, MeetingHandle, MeetingMode, SetMuteArgs,
    SetupStatus, StartChecks, StartMeetingArgs,
};

/// Managed state: the id of the running meeting, if any.
#[derive(Debug, Default)]
pub struct MeetingState {
    running: Mutex<Option<String>>,
}

impl MeetingState {
    /// The state is a plain `Option`, so a panic in another thread cannot leave
    /// it half-written: a poisoned lock is still safe to read and to write.
    fn lock(&self) -> MutexGuard<'_, Option<String>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// True while a meeting runs.
    pub fn is_running(&self) -> bool {
        self.lock().is_some()
    }
}

/// Parse the wire value of `mode`. Only the four lowercase names are valid.
pub fn parse_mode(mode: &str) -> IpcResult<MeetingMode> {
    match mode {
        "off" => Ok(MeetingMode::Off),
        "listen" => Ok(MeetingMode::Listen),
        "speak" => Ok(MeetingMode::Speak),
        "both" => Ok(MeetingMode::Both),
        _ => Err(IpcError::invalid_input(format!("Unknown mode: {mode}."))),
    }
}

/// Message shown when setup blocks a start.
pub fn setup_refusal_message(status: &SetupStatus) -> String {
    format!(
        "Finish setup first. Not turned on yet: {}.",
        status.missing_groups.join(", ")
    )
}

/// Refuse or begin a meeting. `failed_checks` are the per-meeting checks that
/// failed. Refusal order: MEETING_RUNNING, SETUP_INCOMPLETE, START_CHECK_FAILED.
pub fn begin_meeting(
    state: &MeetingState,
    status: &SetupStatus,
    failed_checks: &[StartCheck],
    _mode: MeetingMode,
    meeting_id: String,
) -> IpcResult<MeetingHandle> {
    let mut running = state.lock();
    if running.is_some() {
        return Err(IpcError::new(
            ErrorCode::MeetingRunning,
            "A meeting is already running.",
        ));
    }
    if !status.can_start {
        return Err(IpcError::new(
            ErrorCode::SetupIncomplete,
            setup_refusal_message(status),
        ));
    }
    if let Some(err) = refusal(failed_checks) {
        return Err(err);
    }
    *running = Some(meeting_id.clone());
    Ok(MeetingHandle { meeting_id })
}

pub fn end_meeting(state: &MeetingState) -> IpcResult<MeetingHandle> {
    let mut running = state.lock();
    match running.take() {
        Some(meeting_id) => Ok(MeetingHandle { meeting_id }),
        None => Err(IpcError::new(
            ErrorCode::NoMeeting,
            "No meeting is running.",
        )),
    }
}

/// Start a meeting from the wire arguments. `meeting://status` `running` goes out
/// only after the start succeeded.
pub fn start(
    state: &MeetingState,
    status: &SetupStatus,
    args: &StartMeetingArgs,
    facts: &dyn AudioFacts,
    meeting_id: String,
    sink: &dyn StatusSink,
) -> IpcResult<MeetingHandle> {
    let mode = parse_mode(&args.mode)?;
    let failed = failed_checks(args, facts);
    let handle = begin_meeting(state, status, &failed, mode, meeting_id)?;
    sink.emit_status(RunState::Running);
    Ok(handle)
}

/// Stop the running meeting. `meeting://status` `idle` goes out only after the stop succeeded.
pub fn stop(state: &MeetingState, sink: &dyn StatusSink) -> IpcResult<MeetingHandle> {
    let handle = end_meeting(state)?;
    sink.emit_status(RunState::Idle);
    Ok(handle)
}

/// Mute or unmute one direction. Needs a running meeting (`NO_MEETING` without
/// one). The audio path does not exist yet, so a mute request cannot take
/// effect: it answers `NOT_IMPLEMENTED` and never `Ok`, because an `Ok` would
/// tell the owner that the microphone is muted when it is not.
pub fn apply_mute(state: &MeetingState, _args: &SetMuteArgs) -> IpcResult<()> {
    if !state.is_running() {
        return Err(IpcError::new(
            ErrorCode::NoMeeting,
            "No meeting is running.",
        ));
    }
    Err(IpcError::not_implemented("Muting"))
}

/// Start a meeting on the setup status and audio facts of this machine. Device
/// detection does not exist yet, so a default build answers `NOT_IMPLEMENTED`:
/// nothing starts, no state changes and no event goes out. Tests and a
/// `demo-data` build use sample facts.
#[cfg(any(test, feature = "demo-data"))]
pub fn start_on_detected_facts(
    state: &MeetingState,
    args: &StartMeetingArgs,
    meeting_id: String,
    sink: &dyn StatusSink,
) -> IpcResult<MeetingHandle> {
    start(
        state,
        &sample_setup_status(),
        args,
        &SampleAudioFacts,
        meeting_id,
        sink,
    )
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn start_on_detected_facts(
    _state: &MeetingState,
    _args: &StartMeetingArgs,
    _meeting_id: String,
    _sink: &dyn StatusSink,
) -> IpcResult<MeetingHandle> {
    Err(IpcError::not_implemented("Starting a meeting"))
}

#[tauri::command]
pub async fn start_meeting(
    app: AppHandle,
    state: State<'_, MeetingState>,
    mode: String,
    meeting_app_id: String,
    checks: StartChecks,
) -> IpcResult<MeetingHandle> {
    let args = StartMeetingArgs {
        mode,
        meeting_app_id,
        checks,
    };
    start_on_detected_facts(&state, &args, unique_id("meeting", now_ms()), &app)
}

#[tauri::command]
pub async fn stop_meeting(
    app: AppHandle,
    state: State<'_, MeetingState>,
) -> IpcResult<MeetingHandle> {
    stop(&state, &app)
}

#[tauri::command]
pub async fn set_mute(
    state: State<'_, MeetingState>,
    direction: Direction,
    muted: bool,
) -> IpcResult<()> {
    let args = SetMuteArgs { direction, muted };
    apply_mute(&state, &args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::SetupState;

    fn ready_status() -> SetupStatus {
        let mut items = sample_setup_status().items;
        for item in &mut items {
            item.state = SetupState::Ok;
        }
        SetupStatus::from_items(items)
    }

    #[test]
    fn start_is_refused_with_sample_setup_data() {
        let state = MeetingState::default();
        let err = begin_meeting(
            &state,
            &sample_setup_status(),
            &[],
            MeetingMode::Both,
            "m1".to_string(),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::SetupIncomplete);
        assert_eq!(
            err.message,
            "Finish setup first. Not turned on yet: Models."
        );
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "SETUP_INCOMPLETE");
    }

    #[test]
    fn setup_refusal_message_is_never_empty_for_an_empty_item_list() {
        let status = SetupStatus::from_items(Vec::new());
        assert_eq!(
            setup_refusal_message(&status),
            "Finish setup first. Not turned on yet: Microphone, Meeting audio, \
             Virtual microphone, Models."
        );
    }

    #[test]
    fn setup_refusal_message_lists_microphone_and_models_in_order() {
        let mut items = sample_setup_status().items;
        items[0].state = SetupState::Missing;
        let status = SetupStatus::from_items(items);
        assert_eq!(
            setup_refusal_message(&status),
            "Finish setup first. Not turned on yet: Microphone, Models."
        );
    }

    #[test]
    fn start_then_stop_round_trip() {
        let state = MeetingState::default();
        let handle = begin_meeting(
            &state,
            &ready_status(),
            &[],
            MeetingMode::Listen,
            "m1".into(),
        )
        .unwrap();
        assert_eq!(handle.meeting_id, "m1");
        assert_eq!(serde_json::to_value(&handle).unwrap()["meetingId"], "m1");
        let again = begin_meeting(
            &state,
            &ready_status(),
            &[],
            MeetingMode::Listen,
            "m2".into(),
        )
        .unwrap_err();
        assert_eq!(again.code, ErrorCode::MeetingRunning);
        assert_eq!(end_meeting(&state).unwrap().meeting_id, "m1");
        assert_eq!(end_meeting(&state).unwrap_err().code, ErrorCode::NoMeeting);
    }

    use crate::events::{RunState, StatusSink};
    use crate::ipc::{SetMuteArgs, StartMeetingArgs};
    use std::cell::RefCell;

    #[derive(Default)]
    struct Recorder(RefCell<Vec<RunState>>);

    impl StatusSink for Recorder {
        fn emit_status(&self, state: RunState) {
            self.0.borrow_mut().push(state);
        }
    }

    fn start_args(mode: &str) -> StartMeetingArgs {
        StartMeetingArgs {
            mode: mode.to_string(),
            meeting_app_id: "sample-meeting-app".to_string(),
            checks: StartChecks {
                headphones_confirmed: true,
                meeting_mic_confirmed: true,
            },
        }
    }

    fn unconfirmed_args() -> StartMeetingArgs {
        StartMeetingArgs {
            meeting_app_id: " ".to_string(),
            checks: StartChecks {
                headphones_confirmed: false,
                meeting_mic_confirmed: false,
            },
            ..start_args("both")
        }
    }

    const SAMPLE: SampleAudioFacts = SampleAudioFacts;

    #[test]
    fn mode_parses_the_four_lowercase_values_only() {
        for (text, mode) in [
            ("off", MeetingMode::Off),
            ("listen", MeetingMode::Listen),
            ("speak", MeetingMode::Speak),
            ("both", MeetingMode::Both),
        ] {
            assert_eq!(parse_mode(text).unwrap(), mode);
        }
        for bad in ["", "loud", "Both", "BOTH", " both", "both "] {
            let err = parse_mode(bad).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "mode {bad:?}");
        }
    }

    #[test]
    fn start_with_unknown_mode_is_invalid_input_and_changes_nothing() {
        let state = MeetingState::default();
        let rec = Recorder::default();
        let err = start(
            &state,
            &ready_status(),
            &start_args("loud"),
            &SAMPLE,
            "m1".into(),
            &rec,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(!state.is_running());
        assert!(rec.0.borrow().is_empty());
    }

    #[test]
    fn start_emits_running_after_success_and_stop_emits_idle() {
        let state = MeetingState::default();
        let rec = Recorder::default();
        let handle = start(
            &state,
            &ready_status(),
            &start_args("both"),
            &SAMPLE,
            "m1".into(),
            &rec,
        )
        .unwrap();
        assert_eq!(handle.meeting_id, "m1");
        assert_eq!(*rec.0.borrow(), [RunState::Running]);
        assert_eq!(stop(&state, &rec).unwrap().meeting_id, "m1");
        assert_eq!(*rec.0.borrow(), [RunState::Running, RunState::Idle]);
    }

    #[test]
    fn failed_start_and_failed_stop_emit_nothing() {
        let state = MeetingState::default();
        let rec = Recorder::default();
        let err = start(
            &state,
            &sample_setup_status(),
            &start_args("both"),
            &SAMPLE,
            "m1".into(),
            &rec,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::SetupIncomplete);
        assert_eq!(stop(&state, &rec).unwrap_err().code, ErrorCode::NoMeeting);
        assert!(rec.0.borrow().is_empty());
    }

    fn speakers() -> impl AudioFacts {
        struct OnSpeakers;
        impl AudioFacts for OnSpeakers {
            fn output_device(&self) -> crate::commands::start_checks::OutputDevice {
                crate::commands::start_checks::OutputDevice::BuiltInSpeakers
            }
            fn capture_and_voice_share_device(&self) -> bool {
                false
            }
        }
        OnSpeakers
    }

    #[test]
    fn failed_checks_refuse_the_start_and_change_nothing() {
        let state = MeetingState::default();
        let rec = Recorder::default();
        let err = start(
            &state,
            &ready_status(),
            &unconfirmed_args(),
            &speakers(),
            "m1".into(),
            &rec,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::StartCheckFailed);
        assert_eq!(
            err.details,
            Some(vec![
                "headphones".to_string(),
                "meetingMic".to_string(),
                "meetingApp".to_string()
            ])
        );
        assert!(!state.is_running());
        assert!(rec.0.borrow().is_empty());
    }

    #[test]
    fn confirming_the_checks_lets_the_start_through() {
        let state = MeetingState::default();
        let rec = Recorder::default();
        let handle = start(
            &state,
            &ready_status(),
            &start_args("both"),
            &speakers(),
            "m1".into(),
            &rec,
        )
        .unwrap();
        assert_eq!(handle.meeting_id, "m1");
        assert!(state.is_running());
    }

    #[test]
    fn refusal_order_is_mode_then_running_then_setup_then_checks() {
        let state = MeetingState::default();
        let rec = Recorder::default();
        let sample = sample_setup_status();
        let bad_checks = unconfirmed_args();

        let mut bad_mode = unconfirmed_args();
        bad_mode.mode = "loud".to_string();
        let err = start(&state, &sample, &bad_mode, &speakers(), "m1".into(), &rec).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let err = start(&state, &sample, &bad_checks, &speakers(), "m1".into(), &rec).unwrap_err();
        assert_eq!(err.code, ErrorCode::SetupIncomplete);

        start(
            &state,
            &ready_status(),
            &start_args("both"),
            &SAMPLE,
            "m1".into(),
            &rec,
        )
        .unwrap();
        let err = start(&state, &sample, &bad_checks, &speakers(), "m2".into(), &rec).unwrap_err();
        assert_eq!(err.code, ErrorCode::MeetingRunning);
        assert_eq!(err.details, None);
    }

    #[test]
    fn set_mute_without_a_meeting_is_no_meeting() {
        let state = MeetingState::default();
        let args = SetMuteArgs {
            direction: Direction::Out,
            muted: true,
        };
        assert_eq!(
            apply_mute(&state, &args).unwrap_err().code,
            ErrorCode::NoMeeting
        );
    }

    #[test]
    fn set_mute_during_a_meeting_is_not_implemented_never_ok() {
        let state = MeetingState::default();
        begin_meeting(&state, &ready_status(), &[], MeetingMode::Both, "m1".into()).unwrap();
        for direction in [Direction::In, Direction::Out] {
            for muted in [true, false] {
                let err = apply_mute(&state, &SetMuteArgs { direction, muted }).unwrap_err();
                assert_eq!(err.code, ErrorCode::NotImplemented);
                assert_eq!(err.message, "Muting is not available yet.");
            }
        }
        assert!(
            state.is_running(),
            "a refused mute must not end the meeting"
        );
    }

    #[test]
    fn a_poisoned_meeting_state_still_answers_and_still_changes() {
        let state = MeetingState::default();
        begin_meeting(&state, &ready_status(), &[], MeetingMode::Both, "m1".into()).unwrap();
        let poisoner = std::thread::scope(|s| {
            s.spawn(|| {
                let _guard = state.running.lock().unwrap();
                panic!("poison the meeting state on purpose");
            })
            .join()
        });
        assert!(poisoner.is_err());
        assert!(state.running.is_poisoned());
        assert!(state.is_running());
        assert_eq!(end_meeting(&state).unwrap().meeting_id, "m1");
        assert!(!state.is_running());
        begin_meeting(&state, &ready_status(), &[], MeetingMode::Both, "m2".into()).unwrap();
        assert!(state.is_running());
    }
}
