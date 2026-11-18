//! Capture plan and output check: when a session must be refused.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use audio_io::{
    capture_plan, check_listening_device, CaptureError, CaptureMethod, DeviceInfo, DeviceKind,
    OutputError,
};

#[test]
fn no_process_tap_and_no_2ch_device_refuses_the_session() {
    assert_eq!(
        capture_plan(false, "meeting.app", None),
        Err(CaptureError::NoCaptureRoute)
    );
}

#[test]
fn a_device_that_is_not_a_2ch_virtual_input_does_not_open_a_route() {
    for kind in [
        DeviceKind::Microphone,
        DeviceKind::Headphones,
        DeviceKind::BuiltInSpeakers,
        DeviceKind::VirtualOutput16ch,
        DeviceKind::Other,
    ] {
        let device = DeviceInfo::new("d1", "Some device", kind);
        assert_eq!(
            capture_plan(false, "meeting.app", Some(&device)),
            Err(CaptureError::NoCaptureRoute),
            "{kind:?}"
        );
    }
}

#[test]
fn the_2ch_virtual_device_is_used_when_the_tap_is_missing() {
    let device = DeviceInfo::new("v2", "Virtual 2ch", DeviceKind::VirtualInput2ch);
    assert_eq!(
        capture_plan(false, "meeting.app", Some(&device)),
        Ok(CaptureMethod::VirtualDevice {
            device_id: "v2".into()
        })
    );
}

#[test]
fn built_in_speakers_are_refused_without_confirmed_headphones() {
    assert_eq!(
        check_listening_device(DeviceKind::BuiltInSpeakers, false),
        Err(OutputError::SpeakersNeedHeadphones)
    );
    assert_eq!(
        check_listening_device(DeviceKind::BuiltInSpeakers, true),
        Ok(())
    );
}

#[test]
fn the_virtual_microphone_is_never_a_listening_device() {
    // Listening on the virtual microphone would send the partner's voice back
    // into the meeting, with or without confirmed headphones.
    for headphones_confirmed in [false, true] {
        assert_eq!(
            check_listening_device(DeviceKind::VirtualOutput16ch, headphones_confirmed),
            Err(OutputError::VirtualMicrophoneLoop)
        );
    }
}

#[test]
fn the_process_tap_wins_over_the_2ch_virtual_device() {
    let v2 = DeviceInfo::new("v2", "Virtual 2ch", DeviceKind::VirtualInput2ch);
    assert_eq!(
        capture_plan(true, "meeting.app", Some(&v2)),
        Ok(CaptureMethod::ProcessTap {
            app_id: "meeting.app".into()
        })
    );
}

#[test]
fn a_tap_without_a_chosen_app_falls_back_to_the_device_or_refuses() {
    let v2 = DeviceInfo::new("v2", "Virtual 2ch", DeviceKind::VirtualInput2ch);
    assert_eq!(
        capture_plan(true, "", Some(&v2)),
        Ok(CaptureMethod::VirtualDevice {
            device_id: "v2".into()
        })
    );
    assert_eq!(
        capture_plan(true, "", None),
        Err(CaptureError::NoCaptureRoute)
    );
}
