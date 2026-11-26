//! The criteria of a session: its load, its start, its stop and its failures
//! (SL-01, AC-12 to AC-27, AC-36, AC-38, AC-39).

use std::fs;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use audio_io::{DeviceInfo, DeviceKind, MicConfig, MicPermission, Transport};
use meeting_core::egress::test_support::{FakeTransport, RecordingLog};
use meeting_core::egress::{GuardedClient, Purpose, Request};
use model_store::DownloadRegistry;

use crate::commands::models::request_download_with;
use crate::events::{BannerKind, RunState};
use crate::ipc::SetupState::{Error, Missing, Ok as Ready};
use crate::ipc::{Direction, ErrorCode, IpcError, IpcResult, MeetingHandle, SetMuteArgs};
use crate::native_confirm;
use crate::session::testing::{
    model_states, start_args, until, working_devices, Load, Mode, Rig, Sent,
};

const MODELS_FAILED: &str = "The speech models could not be loaded.";
const FILE_FAILED: &str = "A model file failed its check.";
const MIC_FAILED: &str = "The microphone could not be opened.";
const STOPPED: &str = "Speech processing stopped. Press Start to try again.";
const LOST: &str = "Microphone disconnected. Your outgoing direction is stopped. \
                    Reconnect it, then press Start.";

use RunState::{Idle, Running, Starting};

fn refusal(result: IpcResult<MeetingHandle>) -> (ErrorCode, String) {
    let IpcError { code, message, .. } = result.expect_err("the start is not refused");
    (code, message)
}

fn failed(message: &str) -> (ErrorCode, String) {
    (ErrorCode::StartFailed, message.to_string())
}

/// Speak one sentence into the n-th microphone and wait for the n-th caption.
fn caption_one(rig: &Rig, microphone: usize, captions: usize) {
    rig.fake.feed(microphone).utterance();
    until("the caption", || rig.events.captions().len() >= captions);
}

fn wait_for_banner(rig: &Rig) {
    until("the banner", || !rig.events.banners().is_empty());
}

fn active(rig: &Rig) -> bool {
    rig.session.privacy_policy().is_session_active()
}

/// Sends one model download to a pinned host through the guard, under the policy
/// of the app. Returns how many requests reached the transport.
fn guarded_attempts(rig: &Rig) -> usize {
    let transport = FakeTransport::ok();
    let guard = GuardedClient::new(transport.clone(), RecordingLog::new());
    let host = model_store::pinned_hosts().first().expect("a pinned host");
    let request = Request::new(Purpose::ModelDownload, format!("https://{host}/file"));
    let sent = guard.get(&rig.session.privacy_policy(), &request).is_ok();
    assert_eq!(sent, !transport.calls().is_empty());
    transport.calls().len()
}

/// The working computer with another default output.
fn with_output(output: Option<DeviceKind>) -> Vec<DeviceInfo> {
    let mut devices = working_devices();
    devices.retain(|device| device.kind != DeviceKind::Headphones);
    if let Some(kind) = output {
        devices.push(
            DeviceInfo::new("Out", "Out", kind)
                .with_channels(0, 2)
                .with_transport(Transport::BuiltIn)
                .with_defaults(false, true),
        );
    }
    devices
}

#[test]
fn file_changed_after_launch_is_not_loaded() {
    for removed in [false, true] {
        let rig = Rig::new();
        assert_eq!(
            model_states(&rig.session.check_setup()),
            [Ready, Ready, Ready, Ready]
        );
        let file = rig.folder.at("asr/vad/vad.onnx");
        if removed {
            fs::remove_file(file).unwrap();
        } else {
            fs::write(file, b"abd").unwrap();
        }
        assert_eq!(refusal(rig.start()), failed(FILE_FAILED));
        assert!(rig.fake.loads.lock().unwrap().is_empty());
        assert_eq!(rig.events.statuses(), [Starting, Idle]);
        let sr = if removed { Missing } else { Error };
        assert_eq!(
            model_states(&rig.session.check_setup()),
            [sr, Ready, Ready, Ready]
        );
    }
}

#[test]
fn models_load_once_per_app_run() {
    let rig = Rig::new();
    rig.start().unwrap();
    caption_one(&rig, 0, 1);
    rig.stop().unwrap();
    // Once at launch and once before the load.
    assert_eq!(rig.hashed("asr/a.bin"), 2);
    rig.start().unwrap();
    caption_one(&rig, 1, 2);
    assert_eq!(
        *rig.fake.loads.lock().unwrap(),
        [rig.folder.path().to_path_buf()]
    );
    assert_eq!(rig.hashed("asr/a.bin"), 2);
    assert_eq!(rig.hashed("asr/vad/vad.onnx"), 2);
    let calls = rig.fake.engines.detector_calls.lock().unwrap().clone();
    assert_eq!(
        calls.iter().filter(|reset| **reset).count(),
        2,
        "one reset in each session"
    );
    assert!(calls[0], "the reset comes before the first frame");
}

#[test]
fn failed_load_is_tried_again() {
    for plan in [Load::Fail, Load::Panic] {
        let rig = Rig::new();
        rig.fake.next_loads.lock().unwrap().push_back(plan);
        assert_eq!(refusal(rig.start()), failed(MODELS_FAILED), "{plan:?}");
        assert_eq!(rig.events.statuses(), [Starting, Idle]);
        assert_eq!(rig.hashed("asr/a.bin"), 2);
        rig.start().unwrap();
        assert_eq!(rig.hashed("asr/a.bin"), 3);
        assert_eq!(rig.fake.loads.lock().unwrap().len(), 2);
        assert_eq!(rig.events.statuses(), [Starting, Idle, Starting, Running]);
    }
}

#[test]
fn load_error_names_no_folder() {
    let rig = Rig::new();
    rig.fake.next_loads.lock().unwrap().push_back(Load::Fail);
    let (_, message) = refusal(rig.start());
    assert!(!message.contains(&rig.folder.path().display().to_string()));
    assert_eq!(message, MODELS_FAILED);
}

#[test]
fn remote_engine_is_refused_by_the_registry() {
    let rig = Rig::new();
    rig.fake.next_loads.lock().unwrap().push_back(Load::Remote);
    assert_eq!(refusal(rig.start()), failed(MODELS_FAILED));
    assert!(rig.fake.engines.detector_calls.lock().unwrap().is_empty());
    assert_eq!(rig.fake.engines.recognizer_calls.load(Ordering::SeqCst), 0);
    assert!(rig.fake.opened.lock().unwrap().is_empty());
}

#[test]
fn start_refusals_run_in_the_fixed_order() {
    let rig = Rig::new();
    let deny = || *rig.fake.permission.lock().unwrap() = MicPermission::Denied;
    deny();
    let code = |mode, headphones| refusal(rig.start_with(&start_args(mode, headphones)));
    assert_eq!(code("loud", false).0, ErrorCode::InvalidInput);
    for mode in ["off", "listen"] {
        assert_eq!(
            code(mode, false),
            (
                ErrorCode::NotImplemented,
                "This mode is not available yet.".to_string()
            )
        );
    }
    *rig.fake.devices.lock().unwrap() = with_output(Some(DeviceKind::BuiltInSpeakers));
    assert_eq!(code("speak", false).0, ErrorCode::SetupIncomplete);
    assert!(
        rig.events.all().is_empty(),
        "a refused start sends no event"
    );

    *rig.fake.permission.lock().unwrap() = MicPermission::Granted;
    let refused = rig.start_with(&start_args("speak", false)).unwrap_err();
    assert_eq!(refused.code, ErrorCode::StartCheckFailed);
    assert_eq!(refused.details, Some(vec!["headphones".to_string()]));
    assert!(rig.events.all().is_empty());

    // A session that is starting comes before the setup.
    rig.fake.load_gate.hold();
    thread::scope(|scope| {
        let held = scope.spawn(|| rig.start());
        rig.fake.load_gate.wait_for_arrival(1);
        deny();
        assert_eq!(
            code("speak", false),
            (
                ErrorCode::MeetingRunning,
                "A meeting is already running.".to_string()
            )
        );
        assert_eq!(rig.events.statuses(), [Starting]);
        rig.fake.load_gate.release();
        held.join().unwrap().unwrap();
    });
}

#[test]
fn denied_microphone_blocks_start() {
    let rig = Rig::new();
    *rig.fake.permission.lock().unwrap() = MicPermission::Denied;
    assert_eq!(
        refusal(rig.start()),
        (
            ErrorCode::SetupIncomplete,
            "Finish setup first. Not turned on yet: Microphone.".to_string()
        )
    );
    assert!(rig.fake.opened.lock().unwrap().is_empty());
    assert!(rig.events.all().is_empty());
}

#[test]
fn start_checks_the_permission_while_starting() {
    let rig = Rig::new();
    rig.fake.load_gate.hold();
    thread::scope(|scope| {
        let held = scope.spawn(|| rig.start());
        rig.fake.load_gate.wait_for_arrival(1);
        rig.session.check_setup();
        rig.fake.load_gate.release();
        held.join().unwrap().unwrap();
    });
    rig.session.check_setup();
    // The start, a check while it starts, a check while it runs.
    assert_eq!(*rig.fake.asked.lock().unwrap(), [false, false, true]);
    assert_eq!(rig.fake.probes.load(Ordering::SeqCst), 1);

    // An answer that skipped the microphone is not the answer of this start.
    let rig = Rig::new();
    rig.fake.waits.store(1, Ordering::SeqCst);
    rig.start().unwrap();
    assert_eq!(rig.fake.asked.lock().unwrap().len(), 2);
}

#[test]
fn start_checks_read_the_output_device() {
    for (output, accepted) in [
        (Some(DeviceKind::Headphones), true),
        (Some(DeviceKind::BuiltInSpeakers), false),
        (None, false),
        (Some(DeviceKind::Other), true),
    ] {
        let rig = Rig::new();
        *rig.fake.devices.lock().unwrap() = with_output(output);
        let result = rig.start_with(&start_args("speak", false));
        if accepted {
            assert!(result.is_ok(), "{output:?}: {result:?}");
        } else {
            let refused = result.unwrap_err();
            assert_eq!(refused.code, ErrorCode::StartCheckFailed, "{output:?}");
            assert_eq!(refused.details, Some(vec!["headphones".to_string()]));
        }
        assert_eq!(rig.fake.asked.lock().unwrap().len(), 1, "one device read");
    }
}

#[test]
fn start_opens_the_setup_microphone_after_the_load() {
    for mode in ["speak", "both"] {
        let rig = Rig::new();
        let mut devices = working_devices();
        devices[0] = devices[0].clone().with_defaults(false, false);
        devices[1] = devices[1].clone().with_defaults(true, false);
        *rig.fake.devices.lock().unwrap() = devices;
        let handle = rig.start_with(&start_args(mode, true)).unwrap();
        assert_eq!(handle.meeting_id, "m1");
        assert_eq!(
            rig.fake.journal.all(),
            [
                "status Starting",
                "load start",
                "load end",
                "open",
                "status Running"
            ]
        );
        assert_eq!(
            *rig.fake.opened.lock().unwrap(),
            [MicConfig {
                device_name: Some("Mic".to_string())
            }]
        );
    }
    // The default input is opened without a name.
    let rig = Rig::new();
    rig.start().unwrap();
    assert_eq!(*rig.fake.opened.lock().unwrap(), [MicConfig::default()]);
}

#[test]
fn capture_open_failure_ends_the_session() {
    let rig = Rig::new();
    rig.fake.open_fails.store(true, Ordering::SeqCst);
    assert_eq!(refusal(rig.start()), failed(MIC_FAILED));
    assert_eq!(rig.events.statuses(), [Starting, Idle]);
    assert!(!active(&rig));
    rig.fake.open_fails.store(false, Ordering::SeqCst);
    rig.start().unwrap();
}

#[test]
fn stop_while_starting_cancels_the_start() {
    for held_in in ["setup", "load", "open"] {
        let rig = Rig::new();
        let gate = match held_in {
            "setup" => &rig.fake.setup_gate,
            "load" => &rig.fake.load_gate,
            _ => &rig.fake.open_gate,
        };
        gate.hold();
        thread::scope(|scope| {
            let held = scope.spawn(|| rig.start());
            gate.wait_for_arrival(1);
            // Returns while the start is still held.
            assert_eq!(rig.stop().unwrap().meeting_id, "m1", "{held_in}");
            assert_eq!(rig.events.statuses().last(), Some(&Idle));
            gate.release();
            assert_eq!(
                refusal(held.join().unwrap()),
                (ErrorCode::NoMeeting, "No meeting is running.".to_string()),
                "{held_in}"
            );
        });
        let statuses = rig.events.statuses();
        assert!(!statuses.contains(&Running), "{held_in}: {statuses:?}");
        assert_eq!(statuses.last(), Some(&Idle));
        assert_eq!(rig.fake.open_captures(), 0, "{held_in}");
        assert!(!active(&rig));
    }
}

#[test]
fn new_start_after_a_stop_reuses_pending_work() {
    // A load that has not ended.
    let rig = Rig::new();
    rig.fake.load_gate.hold();
    thread::scope(|scope| {
        let first = scope.spawn(|| rig.start());
        rig.fake.load_gate.wait_for_arrival(1);
        rig.stop().unwrap();
        let second = scope.spawn(|| rig.start());
        until("the second start", || {
            rig.events.statuses() == [Starting, Idle, Starting]
        });
        thread::sleep(Duration::from_millis(100));
        assert!(!second.is_finished(), "it waits for the load");
        rig.fake.load_gate.release();
        assert_eq!(refusal(first.join().unwrap()).0, ErrorCode::NoMeeting);
        second.join().unwrap().unwrap();
    });
    assert_eq!(rig.fake.loads.lock().unwrap().len(), 1);

    // A recognition call that has not returned.
    let rig = Rig::new();
    let gate = &rig.fake.engines.recognizer_gate;
    gate.hold();
    rig.start().unwrap();
    rig.fake.feed(0).utterance();
    gate.wait_for_arrival(1);
    rig.stop().unwrap();
    rig.start().unwrap();
    rig.fake.feed(1).utterance();
    gate.release();
    until("the new caption", || !rig.events.captions().is_empty());
    let texts: Vec<String> = rig
        .events
        .captions()
        .into_iter()
        .map(|c| c.source)
        .collect();
    assert_eq!(texts, ["text 2"]);
}

#[test]
fn no_event_of_a_session_follows_its_stop() {
    let rig = Rig::new();
    let gate = &rig.fake.engines.recognizer_gate;
    gate.hold();
    rig.start().unwrap();
    let feed = rig.fake.feed(0);
    feed.utterance();
    feed.utterance();
    gate.wait_for_arrival(1);
    until("both sentences are read", || {
        rig.fake.engines.detector_calls.lock().unwrap().len() > 40
    });
    // Returns while the call is held.
    rig.stop().unwrap();
    assert_eq!(rig.fake.open_captures(), 0);
    let at_stop = rig.events.all();
    assert_eq!(at_stop.last(), Some(&Sent::Status(Idle)));
    gate.release();
    thread::sleep(Duration::from_millis(200));
    assert_eq!(rig.events.all(), at_stop);
    assert_eq!(
        rig.fake.engines.recognizer_calls.load(Ordering::SeqCst),
        1,
        "the waiting span is not recognized"
    );
}

#[test]
fn session_blocks_egress_from_start_to_end() {
    let rig = Rig::new();
    assert!(!active(&rig));
    assert_eq!(guarded_attempts(&rig), 1, "without a session");
    // A refusal and a failed start leave no session.
    assert!(rig.start_with(&start_args("listen", true)).is_err());
    assert!(!active(&rig));
    *rig.fake.permission.lock().unwrap() = MicPermission::Denied;
    assert!(rig.start().is_err());
    assert!(!active(&rig));
    *rig.fake.permission.lock().unwrap() = MicPermission::Granted;
    rig.fake.open_fails.store(true, Ordering::SeqCst);
    assert!(rig.start().is_err());
    assert!(!active(&rig));
    rig.fake.open_fails.store(false, Ordering::SeqCst);

    rig.fake.open_gate.hold();
    thread::scope(|scope| {
        let held = scope.spawn(|| rig.start());
        rig.fake.open_gate.wait_for_arrival(2);
        assert!(active(&rig), "from the accepted start");
        assert_eq!(guarded_attempts(&rig), 0, "while starting");
        for mode in ["loud", "listen", "speak"] {
            assert!(rig.start_with(&start_args(mode, true)).is_err());
            assert!(active(&rig), "after a refused {mode}");
        }
        rig.fake.open_gate.release();
        held.join().unwrap().unwrap();
    });
    assert!(active(&rig));
    assert_eq!(guarded_attempts(&rig), 0, "while running");
    rig.stop().unwrap();
    assert!(!active(&rig));
    assert_eq!(guarded_attempts(&rig), 1, "after the session");
    assert!(rig.stop().is_err());
    assert!(!active(&rig));

    rig.start().unwrap();
    rig.fake.feed(1).fail();
    wait_for_banner(&rig);
    assert!(!active(&rig));
}

#[test]
fn download_is_refused_during_a_session() {
    let rig = Rig::new();
    let registry = DownloadRegistry::default();
    let folder = rig.folder.at("downloads");
    let meeting = (
        ErrorCode::MeetingRunning,
        "A meeting is running.".to_string(),
    );
    let download = |confirm: &dyn Fn(&native_confirm::ConfirmPrompt)| {
        request_download_with(
            "sr",
            true,
            &registry,
            &folder,
            &|| rig.session.is_active(),
            |prompt| {
                confirm(prompt);
                Some(native_confirm::confirmation_for_tests(&prompt.model_id))
            },
            |_| panic!("no transfer during a session"),
        )
        .map_err(|error| (error.code, error.message))
    };
    rig.start().unwrap();
    assert_eq!(
        download(&|_| panic!("no dialog during a session")),
        Err(meeting.clone())
    );
    rig.stop().unwrap();
    // The dialog is open when a start is accepted; then the owner confirms.
    let confirmed = download(&|_| {
        rig.start().expect("an open dialog does not block a start");
    });
    assert_eq!(confirmed, Err(meeting));
}

#[test]
fn lost_microphone_ends_the_session() {
    let rig = Rig::new();
    rig.start().unwrap();
    rig.fake.feed(0).fail();
    wait_for_banner(&rig);
    let banners = rig.events.banners();
    assert_eq!(banners.len(), 1);
    assert_eq!(
        (banners[0].kind, banners[0].message.as_str()),
        (BannerKind::Error, LOST)
    );
    assert_eq!(rig.events.statuses(), [Starting, Running, Idle]);
    assert_eq!(rig.fake.open_captures(), 0);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(rig.fake.opened.lock().unwrap().len(), 1, "no restart");
    assert_eq!(rig.events.statuses(), [Starting, Running, Idle]);
    assert_eq!(rig.stop().unwrap_err().code, ErrorCode::NoMeeting);
    // The engines are fine, so they are kept.
    rig.start().unwrap();
    assert_eq!(rig.fake.loads.lock().unwrap().len(), 1);
}

#[test]
fn detector_error_ends_the_session() {
    let rig = Rig::new();
    *rig.fake.engines.detector.lock().unwrap() = Mode::Fail;
    rig.start().unwrap();
    rig.fake.feed(0).utterance();
    wait_for_banner(&rig);
    let banners = rig.events.banners();
    assert_eq!(
        (banners[0].kind, banners[0].message.as_str()),
        (BannerKind::Error, STOPPED)
    );
    assert_eq!(rig.events.statuses(), [Starting, Running, Idle]);
    assert_eq!(rig.fake.open_captures(), 0);
    // The engines of a failed session are dropped.
    *rig.fake.engines.detector.lock().unwrap() = Mode::Work;
    rig.start().unwrap();
    assert_eq!(rig.fake.loads.lock().unwrap().len(), 2);
}

/// No session is left to hear about a panic in a call that outlives its own.
#[test]
fn a_panic_after_stop_loads_new_engines() {
    let rig = Rig::new();
    let gate = &rig.fake.engines.recognizer_gate;
    gate.hold();
    rig.start().unwrap();
    rig.fake.feed(0).utterance();
    gate.wait_for_arrival(1);
    rig.stop().unwrap();
    *rig.fake.engines.recognizer.lock().unwrap() = Mode::Panic;
    gate.release();
    thread::sleep(Duration::from_millis(200));
    assert!(rig.events.banners().is_empty());
    rig.start().unwrap();
    assert_eq!(rig.fake.loads.lock().unwrap().len(), 2);
}

#[test]
fn worker_failure_ends_the_session_not_the_app() {
    for worker in ["recognizer", "detector"] {
        let rig = Rig::new();
        let mode = if worker == "recognizer" {
            &rig.fake.engines.recognizer
        } else {
            &rig.fake.engines.detector
        };
        *mode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Mode::Panic;
        rig.start().unwrap();
        rig.fake.feed(0).utterance();
        wait_for_banner(&rig);
        assert_eq!(rig.events.banners()[0].message, STOPPED, "{worker}");
        assert_eq!(rig.events.statuses(), [Starting, Running, Idle]);
        assert_eq!(rig.fake.open_captures(), 0);
        *mode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Mode::Work;
        rig.start().unwrap();
        assert_eq!(rig.hashed("asr/a.bin"), 3, "{worker}: checked again");
        assert_eq!(rig.fake.loads.lock().unwrap().len(), 2);
        assert_eq!(rig.events.statuses().last(), Some(&Running));
        caption_one(&rig, 1, 1);
    }
}

#[test]
fn a_caption_goes_out_as_one_final_outgoing_line() {
    let rig = Rig::new();
    rig.start().unwrap();
    caption_one(&rig, 0, 1);
    until("the latency", || {
        matches!(rig.events.all().last(), Some(Sent::Latency(_)))
    });
    let caption = rig.events.captions().remove(0);
    let number = caption.line_id.strip_prefix("line-").unwrap();
    assert!(number.parse::<u64>().is_ok(), "{}", caption.line_id);
    assert_eq!(caption.direction, Direction::Out);
    assert!(caption.is_final);
    assert_eq!(
        (caption.source.as_str(), caption.target.as_str()),
        ("text 1", "")
    );
    assert!(caption.delay_ms < 5_000);
    let Some(Sent::Latency(latency)) = rig.events.all().pop() else {
        panic!("a latency event follows the caption");
    };
    assert_eq!(
        (latency.p50_ms, latency.p95_ms),
        (caption.delay_ms, caption.delay_ms)
    );
}

#[test]
fn a_failed_sentence_shows_a_warning_and_keeps_the_session() {
    let rig = Rig::new();
    *rig.fake.engines.recognizer.lock().unwrap() = Mode::Fail;
    rig.start().unwrap();
    rig.fake.feed(0).utterance();
    wait_for_banner(&rig);
    let banners = rig.events.banners();
    assert_eq!(
        (banners[0].kind, banners[0].message.as_str()),
        (BannerKind::Warning, "One sentence could not be recognized.")
    );
    assert_eq!(rig.events.statuses(), [Starting, Running]);
    assert!(rig.session.is_active());
}

#[test]
fn mute_needs_a_running_session() {
    let rig = Rig::new();
    let mute = || {
        let args = SetMuteArgs {
            direction: Direction::Out,
            muted: true,
        };
        rig.session.mute(&args).unwrap_err()
    };
    assert_eq!(mute().code, ErrorCode::NoMeeting);
    rig.start().unwrap();
    assert_eq!(mute().message, "Muting is not available yet.");
    rig.stop().unwrap();
    assert_eq!(mute().code, ErrorCode::NoMeeting);
}
