//! Per-meeting start checks. Layer 2 of the start gate: the owner confirms them
//! at every Start. Pure functions over injected facts, so tests need no audio
//! stack and no Tauri runtime.

use crate::ipc::{IpcError, StartMeetingArgs};

/// One per-meeting check. `id()` is the wire string used in `IpcError.details`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartCheck {
    Headphones,
    MeetingMic,
    MeetingApp,
    Routing,
}

impl StartCheck {
    /// Fixed order of `details` and of the message list.
    pub const ORDER: [StartCheck; 4] = [
        Self::Headphones,
        Self::MeetingMic,
        Self::MeetingApp,
        Self::Routing,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Headphones => "headphones",
            Self::MeetingMic => "meetingMic",
            Self::MeetingApp => "meetingApp",
            Self::Routing => "routing",
        }
    }

    /// Text in the refusal message.
    fn text(self) -> &'static str {
        match self {
            Self::Headphones => "headphones in use",
            Self::MeetingMic => "meeting app microphone set to Interlo",
            Self::MeetingApp => "meeting app chosen",
            Self::Routing => "capture and voice output on separate devices",
        }
    }
}

/// Kind of the current audio output device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputDevice {
    BuiltInSpeakers,
    Headphones,
    Other,
}

/// Facts about the audio setup that no argument carries. The shell supplies the
/// real values later; tests pass fixed ones.
pub trait AudioFacts {
    fn output_device(&self) -> OutputDevice;
    /// True when the capture route and the English-voice output use one virtual device.
    fn capture_and_voice_share_device(&self) -> bool;
}

/// Demo facts: headphones, separate devices. Only tests and a `demo-data` build
/// have them; a default build has no device detection and starts no meeting.
#[cfg(any(test, feature = "demo-data"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct SampleAudioFacts;

#[cfg(any(test, feature = "demo-data"))]
impl AudioFacts for SampleAudioFacts {
    fn output_device(&self) -> OutputDevice {
        OutputDevice::Headphones
    }

    fn capture_and_voice_share_device(&self) -> bool {
        false
    }
}

/// The checks that fail, in `StartCheck::ORDER`.
pub fn failed_checks(args: &StartMeetingArgs, facts: &dyn AudioFacts) -> Vec<StartCheck> {
    StartCheck::ORDER
        .into_iter()
        .filter(|check| match check {
            StartCheck::Headphones => {
                facts.output_device() == OutputDevice::BuiltInSpeakers
                    && !args.checks.headphones_confirmed
            }
            StartCheck::MeetingMic => !args.checks.meeting_mic_confirmed,
            StartCheck::MeetingApp => {
                meeting_core::start_checks::check_meeting_app_id(&args.meeting_app_id).is_err()
            }
            StartCheck::Routing => facts.capture_and_voice_share_device(),
        })
        .collect()
}

/// `START_CHECK_FAILED` for a non-empty list, `None` for an empty one.
pub fn refusal(failed: &[StartCheck]) -> Option<IpcError> {
    if failed.is_empty() {
        return None;
    }
    let list: Vec<&str> = failed.iter().map(|c| c.text()).collect();
    Some(IpcError::start_check_failed(
        format!("Confirm before starting: {}.", list.join(", ")),
        failed.iter().map(|c| c.id().to_string()).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{ErrorCode, StartChecks, StartMeetingArgs};

    struct Facts {
        output: OutputDevice,
        shared: bool,
    }

    impl AudioFacts for Facts {
        fn output_device(&self) -> OutputDevice {
            self.output
        }
        fn capture_and_voice_share_device(&self) -> bool {
            self.shared
        }
    }

    const FINE: Facts = Facts {
        output: OutputDevice::Headphones,
        shared: false,
    };

    fn args(app: &str, headphones: bool, mic: bool) -> StartMeetingArgs {
        StartMeetingArgs {
            mode: "both".to_string(),
            meeting_app_id: app.to_string(),
            checks: StartChecks {
                headphones_confirmed: headphones,
                meeting_mic_confirmed: mic,
            },
        }
    }

    fn ids(failed: &[StartCheck]) -> Vec<&'static str> {
        failed.iter().map(|c| c.id()).collect()
    }

    #[test]
    fn nothing_fails_when_every_check_is_satisfied() {
        let failed = failed_checks(&args("app", true, true), &FINE);
        assert!(failed.is_empty());
        assert!(refusal(&failed).is_none());
    }

    #[test]
    fn headphones_fail_on_built_in_speakers_until_confirmed() {
        let speakers = Facts {
            output: OutputDevice::BuiltInSpeakers,
            shared: false,
        };
        assert_eq!(
            ids(&failed_checks(&args("app", false, true), &speakers)),
            ["headphones"]
        );
        assert!(failed_checks(&args("app", true, true), &speakers).is_empty());
    }

    #[test]
    fn headphones_pass_on_other_output_devices_even_when_unconfirmed() {
        for output in [OutputDevice::Headphones, OutputDevice::Other] {
            let facts = Facts {
                output,
                shared: false,
            };
            assert!(
                failed_checks(&args("app", false, true), &facts).is_empty(),
                "{output:?}"
            );
        }
    }

    #[test]
    fn meeting_mic_fails_until_confirmed() {
        assert_eq!(
            ids(&failed_checks(&args("app", true, false), &FINE)),
            ["meetingMic"]
        );
    }

    #[test]
    fn meeting_app_fails_when_blank_after_trim() {
        for blank in ["", " ", "   ", "\t\n "] {
            assert_eq!(
                ids(&failed_checks(&args(blank, true, true), &FINE)),
                ["meetingApp"],
                "{blank:?}"
            );
        }
        for named in ["a", " a ", "com.example.app"] {
            assert!(
                failed_checks(&args(named, true, true), &FINE).is_empty(),
                "{named:?}"
            );
        }
    }

    #[test]
    fn routing_fails_when_capture_and_voice_share_one_device() {
        let shared = Facts {
            output: OutputDevice::Headphones,
            shared: true,
        };
        assert_eq!(
            ids(&failed_checks(&args("app", true, true), &shared)),
            ["routing"]
        );
    }

    #[test]
    fn failed_checks_come_in_the_fixed_order() {
        let all = Facts {
            output: OutputDevice::BuiltInSpeakers,
            shared: true,
        };
        let failed = failed_checks(&args("  ", false, false), &all);
        assert_eq!(
            ids(&failed),
            ["headphones", "meetingMic", "meetingApp", "routing"]
        );
        assert_eq!(failed, StartCheck::ORDER);
    }

    #[test]
    fn refusal_message_lists_the_failed_checks_and_details_hold_their_ids() {
        let err = refusal(&[StartCheck::Headphones, StartCheck::MeetingMic]).unwrap();
        assert_eq!(err.code, ErrorCode::StartCheckFailed);
        assert_eq!(
            err.message,
            "Confirm before starting: headphones in use, meeting app microphone set to Interlo."
        );
        assert_eq!(
            err.details,
            Some(vec!["headphones".into(), "meetingMic".into()])
        );

        let err = refusal(&StartCheck::ORDER).unwrap();
        assert_eq!(
            err.message,
            "Confirm before starting: headphones in use, \
             meeting app microphone set to Interlo, meeting app chosen, \
             capture and voice output on separate devices."
        );
        assert_eq!(
            err.details,
            Some(vec![
                "headphones".into(),
                "meetingMic".into(),
                "meetingApp".into(),
                "routing".into()
            ])
        );

        let err = refusal(&[StartCheck::Routing]).unwrap();
        assert_eq!(
            err.message,
            "Confirm before starting: capture and voice output on separate devices."
        );
    }

    #[test]
    fn sample_facts_let_the_sample_flow_pass() {
        let facts = SampleAudioFacts;
        assert_eq!(facts.output_device(), OutputDevice::Headphones);
        assert!(!facts.capture_and_voice_share_device());
        assert!(failed_checks(&args("app", false, true), &facts).is_empty());
    }
}
