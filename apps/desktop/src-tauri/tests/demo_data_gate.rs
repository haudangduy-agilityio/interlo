//! Sample data never reaches a default build. Two layers:
//! 1. a source scan: every use of a `sample_*` helper or `SampleAudioFacts` in
//!    `src/commands/*.rs` sits inside an item gated by `cfg(test)` or by the
//!    `demo-data` feature;
//! 2. behaviour: in a build without `demo-data`, every command that stood on
//!    sample data answers `NOT_IMPLEMENTED` and changes nothing.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

/// The two gates that keep an item out of a default build.
fn is_demo_gate(attribute: &str) -> bool {
    let compact: String = attribute.split_whitespace().collect();
    compact == "#[cfg(test)]" || compact == r#"#[cfg(any(test,feature="demo-data"))]"#
}

/// Net `{` minus `}` on one line, ignoring string literals and `//` comments.
fn brace_delta(line: &str) -> i64 {
    let mut delta = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut prev = ' ';
    for c in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == '/' && prev == '/' {
            break;
        } else if c == '{' {
            delta += 1;
        } else if c == '}' {
            delta -= 1;
        }
        prev = c;
    }
    delta
}

fn mentions_sample(line: &str) -> bool {
    line.contains("sample_") || line.contains("SampleAudioFacts")
}

/// Lines (1-based) that use a sample symbol outside a gated item. A gate covers
/// the item that follows it: up to its closing brace, or up to its `;`.
fn ungated_sample_uses(source: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut gate_pending = false;
    let mut in_gated_item = false;
    let mut depth = 0i64;
    let mut seen_open = false;
    for (index, line) in source.lines().enumerate() {
        let text = line.trim();
        if in_gated_item {
            depth += brace_delta(text);
            seen_open |= text.contains('{');
            if (seen_open && depth <= 0) || (!seen_open && text.ends_with(';')) {
                in_gated_item = false;
            }
            continue;
        }
        if text.is_empty() || text.starts_with("//") {
            continue;
        }
        if text.starts_with("#[") {
            gate_pending |= is_demo_gate(text);
            continue;
        }
        if gate_pending {
            gate_pending = false;
            depth = brace_delta(text);
            seen_open = text.contains('{');
            in_gated_item = !((seen_open && depth <= 0) || (!seen_open && text.ends_with(';')));
            continue;
        }
        if mentions_sample(text) {
            found.push((index + 1, text.to_string()));
        }
    }
    found
}

fn command_sources() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut files: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read_to_string(&path).unwrap())
        })
        .collect();
    files.sort();
    files
}

#[test]
fn no_command_uses_sample_data_outside_a_gated_item() {
    let files = command_sources();
    assert!(files.len() >= 8, "expected the command modules: {files:?}");
    let mut sample_files = 0;
    let mut problems = Vec::new();
    for (name, source) in &files {
        if source.lines().any(mentions_sample) {
            sample_files += 1;
        }
        for (line, text) in ungated_sample_uses(source) {
            problems.push(format!("src/commands/{name}:{line}: {text}"));
        }
    }
    assert!(sample_files >= 5, "the scan found no sample code to guard");
    assert!(
        problems.is_empty(),
        "sample data outside `#[cfg(any(test, feature = \"demo-data\"))]`:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_scan_flags_an_ungated_use_and_accepts_a_gated_one() {
    let ungated = "pub async fn check_setup() -> IpcResult<SetupStatus> {\n    Ok(sample_setup_status())\n}\n";
    assert_eq!(ungated_sample_uses(ungated).len(), 1);

    let ungated_struct = "let facts = SampleAudioFacts;\n";
    assert_eq!(ungated_sample_uses(ungated_struct).len(), 1);

    let wrong_gate = "#[cfg(unix)]\nfn demo() -> X {\n    sample_x()\n}\n";
    assert_eq!(ungated_sample_uses(wrong_gate).len(), 1);

    let gated = concat!(
        "#[cfg(any(test, feature = \"demo-data\"))]\n",
        "pub fn current() -> X {\n    sample_x()\n}\n",
        "#[cfg(test)]\n#[allow(clippy::unwrap_used)]\nmod tests {\n    fn t() {\n        sample_x();\n    }\n}\n",
        "#[cfg(any(test, feature = \"demo-data\"))]\nuse crate::a::{\n    sample_x,\n    SampleAudioFacts,\n};\n",
        "#[cfg(test)]\nuse crate::a::sample_x;\n",
    );
    assert!(ungated_sample_uses(gated).is_empty());

    let leaks_after_gate = concat!(
        "#[cfg(test)]\nfn t() {\n    sample_x();\n}\n",
        "fn real() {\n    sample_y();\n}\n",
    );
    assert_eq!(ungated_sample_uses(leaks_after_gate).len(), 1);

    let in_a_comment = "// sample_x is only for tests\n/// SampleAudioFacts\nfn real() {}\n";
    assert!(ungated_sample_uses(in_a_comment).is_empty());
}

/// Behaviour of a default build. Compiled out when `demo-data` is on, because
/// that build is meant to answer with samples (see the second module).
#[cfg(not(feature = "demo-data"))]
mod default_build {
    use interlo_desktop::commands::history::current_meetings;
    use interlo_desktop::commands::models::current_models;
    use interlo_desktop::commands::notes::{current_notes, save_note};
    use interlo_desktop::commands::settings::{apply_setting, current_settings};
    use interlo_desktop::ipc::ErrorCode;
    use serde_json::json;

    #[test]
    fn reads_that_stood_on_sample_data_are_not_implemented() {
        assert_eq!(
            current_models().unwrap_err().code,
            ErrorCode::NotImplemented
        );
        assert_eq!(
            current_meetings().unwrap_err().code,
            ErrorCode::NotImplemented
        );
        assert_eq!(current_notes().unwrap_err().code, ErrorCode::NotImplemented);
        assert_eq!(
            current_settings().unwrap_err().code,
            ErrorCode::NotImplemented
        );
    }

    #[test]
    fn writes_that_kept_nothing_are_not_implemented_after_the_input_check() {
        assert_eq!(
            apply_setting("ducking", &json!(true)).unwrap_err().code,
            ErrorCode::NotImplemented
        );
        assert_eq!(
            apply_setting("ducking", &json!("yes")).unwrap_err().code,
            ErrorCode::InvalidInput
        );
        assert_eq!(
            save_note("hello", Some("work".into()), 1).unwrap_err().code,
            ErrorCode::NotImplemented
        );
        assert_eq!(
            save_note("   ", None, 1).unwrap_err().code,
            ErrorCode::InvalidInput
        );
    }
}

/// A default build reads no model folder, so a Start on a computer with every
/// device names the models.
#[cfg(not(any(feature = "demo-data", feature = "dev-models")))]
mod default_start {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use audio_io::{
        DeviceInfo, DeviceKind, DeviceListError, DeviceSnapshot, MicConfig, MicPermission,
        ProbeEnv, Transport,
    };
    use engine_host::live_chain::MonotonicClock;
    use engine_host::EngineError;
    use interlo_desktop::commands::setup::{SetupAnswer, SetupCheck};
    use interlo_desktop::events::{
        BannerEvent, CaptionEvent, LatencyEvent, RunState, SessionSink, StatusSink,
    };
    use interlo_desktop::ipc::{Direction, ErrorCode, SetMuteArgs, StartChecks, StartMeetingArgs};
    use interlo_desktop::models_dir;
    use interlo_desktop::session::capture::{Microphone, OpenFailed};
    use interlo_desktop::session::machine::{LoadedEngines, Machine};
    use interlo_desktop::session::model_check::ModelCheck;
    use interlo_desktop::session::policy::shared_policy;
    use interlo_desktop::session::Session;

    /// Every device a session needs, and a granted microphone.
    struct Devices;

    impl ProbeEnv for Devices {
        fn read_devices(&mut self) -> Result<DeviceSnapshot, DeviceListError> {
            let device = |name: &str, kind, input, output, how| {
                DeviceInfo::new(name, name, kind)
                    .with_channels(input, output)
                    .with_transport(how)
            };
            Ok(DeviceSnapshot {
                devices: vec![
                    device("Mic", DeviceKind::Microphone, 1, 0, Transport::BuiltIn)
                        .with_defaults(true, false),
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
                ],
                skipped: 0,
            })
        }

        fn probe_microphone(&mut self, _config: &MicConfig) -> MicPermission {
            MicPermission::Granted
        }

        fn tap_available(&mut self) -> bool {
            false
        }
    }

    struct Computer(SetupCheck);

    impl Machine for Computer {
        fn setup(&self, meeting_running: &dyn Fn() -> bool) -> SetupAnswer {
            self.0.answer(&mut Devices, meeting_running)
        }

        fn engines(&self, _folder: &Path) -> Result<LoadedEngines, EngineError> {
            panic!("a default build loads nothing")
        }

        fn open_microphone(&self, _config: MicConfig) -> Result<Microphone, OpenFailed> {
            panic!("a default build opens no capture")
        }
    }

    /// Counts every event.
    #[derive(Default)]
    struct Count(Mutex<u32>);

    impl Count {
        fn add(&self) {
            *self.0.lock().unwrap() += 1;
        }
    }

    impl StatusSink for Count {
        fn emit_status(&self, _state: RunState) {
            self.add();
        }
    }

    impl SessionSink for Count {
        fn emit_caption(&self, _event: CaptionEvent) {
            self.add();
        }

        fn emit_latency(&self, _event: LatencyEvent) {
            self.add();
        }

        fn emit_banner(&self, _event: BannerEvent) {
            self.add();
        }
    }

    /// The session of a default build on a computer with every device.
    fn default_session() -> Session {
        let models = ModelCheck::launch(
            models_dir::from_environment(),
            model_store::packs(),
            rustix::process::getuid().as_raw(),
            Box::new(model_store::Sha256Hasher),
        );
        Session::new(
            Box::new(Computer(SetupCheck::default())),
            models,
            shared_policy(),
            Arc::new(MonotonicClock::default()),
        )
    }

    #[test]
    fn mute_without_a_meeting_stays_no_meeting() {
        let args = SetMuteArgs {
            direction: Direction::Out,
            muted: true,
        };
        assert_eq!(
            default_session().mute(&args).unwrap_err().code,
            ErrorCode::NoMeeting
        );
    }

    #[test]
    fn default_build_start_names_the_missing_models() {
        let session = default_session();
        let args = StartMeetingArgs {
            mode: "speak".to_string(),
            meeting_app_id: "meeting-app".to_string(),
            checks: StartChecks {
                headphones_confirmed: true,
                meeting_mic_confirmed: true,
            },
        };
        let events = Arc::new(Count::default());
        let sink: Arc<dyn SessionSink> = events.clone();
        let error = session.start(&args, "m1".to_string(), sink).unwrap_err();
        assert_eq!(error.code, ErrorCode::SetupIncomplete);
        assert_eq!(
            error.message,
            "Finish setup first. Not turned on yet: Models."
        );
        assert_eq!(*events.0.lock().unwrap(), 0);
        assert!(!session.is_active());
    }
}

/// Behaviour of a `demo-data` build: the samples are there on purpose.
#[cfg(feature = "demo-data")]
mod demo_build {
    use interlo_desktop::commands::models::current_models;
    use interlo_desktop::commands::notes::save_note;
    use interlo_desktop::commands::setup::current_setup_status;

    #[test]
    fn samples_answer_when_the_demo_feature_is_on() {
        assert_eq!(current_setup_status().unwrap().items.len(), 8);
        assert_eq!(current_models().unwrap().len(), 4);
        assert!(save_note("hello", None, 1).is_ok());
    }
}
