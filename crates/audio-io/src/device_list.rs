//! The list of audio devices on this Mac, read from the system and sorted into
//! the device kinds the rest of the app uses.
//!
//! The reader runs one fixed system tool and parses its JSON. It never builds a
//! shell string, never panics on bad output and never logs a device name.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::devices::{DeviceInfo, DeviceKind, Transport};

/// Longest the system tool may run (starting value, to confirm on a slow Mac).
pub const SYSTEM_PROFILER_TIMEOUT_MS: u64 = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeviceListError {
    #[error("the system audio tool was not found")]
    ToolNotFound,
    #[error("the system audio tool failed")]
    ToolFailed,
    #[error("the system audio tool gave output that cannot be read")]
    InvalidOutput,
    #[error("the system audio tool took too long")]
    TimedOut,
}

/// The devices the system reported, plus how many items had no name and were left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceSnapshot {
    pub devices: Vec<DeviceInfo>,
    pub skipped: usize,
}

/// Runs a program with arguments and returns its standard output.
pub trait ToolRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        limit: Duration,
    ) -> Result<Vec<u8>, DeviceListError>;
}

/// Starts the real process. The only place in the crate that starts one.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessRunner;

const POLL_INTERVAL_MS: u64 = 10;

fn spawn_error(e: &std::io::Error) -> DeviceListError {
    if e.kind() == std::io::ErrorKind::NotFound {
        DeviceListError::ToolNotFound
    } else {
        DeviceListError::ToolFailed
    }
}

impl ToolRunner for ProcessRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        limit: Duration,
    ) -> Result<Vec<u8>, DeviceListError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| spawn_error(&e))?;
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(DeviceListError::ToolFailed);
        };
        // Read on its own thread so a full pipe never stops the child.
        let reader = thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stdout.read_to_end(&mut out);
            out
        });
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let out = reader.join().map_err(|_| DeviceListError::ToolFailed)?;
                    return if status.success() {
                        Ok(out)
                    } else {
                        Err(DeviceListError::ToolFailed)
                    };
                }
                Ok(None) if started.elapsed() >= limit => {
                    // The reader thread ends on its own when the pipe closes.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DeviceListError::TimedOut);
                }
                Ok(None) => thread::sleep(Duration::from_millis(POLL_INTERVAL_MS)),
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DeviceListError::ToolFailed);
                }
            }
        }
    }
}

/// Kind rules, first match wins: a virtual device is told by its channel counts,
/// an output by its transport, an input-only device is a microphone.
pub fn classify(name: &str, input: u32, output: u32, transport: Transport) -> DeviceKind {
    match (transport, input, output) {
        (Transport::Virtual, 2, 2) => DeviceKind::VirtualInput2ch,
        (Transport::Virtual, 16, 16) => DeviceKind::VirtualOutput16ch,
        (Transport::Virtual, _, _) => DeviceKind::Other,
        (Transport::Bluetooth | Transport::Usb, _, 1..) => DeviceKind::Headphones,
        (Transport::BuiltIn, _, 1..) => {
            if name.to_lowercase().contains("headphone") {
                DeviceKind::Headphones
            } else {
                DeviceKind::BuiltInSpeakers
            }
        }
        (_, _, 1..) => DeviceKind::Other,
        (_, 1.., 0) => DeviceKind::Microphone,
        _ => DeviceKind::Other,
    }
}

fn transport_of(item: &Value) -> Transport {
    let raw = item
        .get("coreaudio_device_transport")
        .and_then(Value::as_str)
        .unwrap_or("");
    match raw.strip_prefix("coreaudio_device_type_").unwrap_or(raw) {
        "builtin" => Transport::BuiltIn,
        "virtual" => Transport::Virtual,
        "bluetooth" => Transport::Bluetooth,
        "usb" => Transport::Usb,
        "hdmi" | "displayport" => Transport::Display,
        _ => Transport::Other,
    }
}

fn channels_of(item: &Value, key: &str) -> u32 {
    item.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

fn is_yes(item: &Value, key: &str) -> bool {
    item.get(key).and_then(Value::as_str) == Some("spaudio_yes")
}

/// Parses the JSON of `system_profiler SPAudioDataType -json`. An item without a
/// name is counted in `skipped`; a wrong top level is `InvalidOutput`.
pub fn parse_system_profiler(text: &str) -> Result<DeviceSnapshot, DeviceListError> {
    let root: Value = serde_json::from_str(text).map_err(|_| DeviceListError::InvalidOutput)?;
    let groups = root
        .get("SPAudioDataType")
        .and_then(Value::as_array)
        .ok_or(DeviceListError::InvalidOutput)?;
    let mut snapshot = DeviceSnapshot::default();
    for item in groups
        .iter()
        .filter_map(|g| g.get("_items").and_then(Value::as_array))
        .flatten()
    {
        let Some(name) = item.get("_name").and_then(Value::as_str) else {
            snapshot.skipped += 1;
            continue;
        };
        let input = channels_of(item, "coreaudio_device_input");
        let output = channels_of(item, "coreaudio_device_output");
        let transport = transport_of(item);
        snapshot.devices.push(
            DeviceInfo::new(name, name, classify(name, input, output, transport))
                .with_channels(input, output)
                .with_transport(transport)
                .with_defaults(
                    is_yes(item, "coreaudio_default_audio_input_device"),
                    is_yes(item, "coreaudio_default_audio_output_device"),
                ),
        );
    }
    Ok(snapshot)
}

/// Reads the devices of this Mac with the one fixed tool call.
pub fn read_system_devices(runner: &dyn ToolRunner) -> Result<DeviceSnapshot, DeviceListError> {
    let bytes = runner.run(
        "system_profiler",
        &["SPAudioDataType", "-json"],
        Duration::from_millis(SYSTEM_PROFILER_TIMEOUT_MS),
    )?;
    let text = String::from_utf8(bytes).map_err(|_| DeviceListError::InvalidOutput)?;
    parse_system_profiler(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::time::Instant;

    const FIXTURE: &str = include_str!("../../../fixtures/audio/system_profiler_audio.json");

    fn by_name<'a>(snapshot: &'a DeviceSnapshot, name: &str) -> &'a DeviceInfo {
        snapshot
            .devices
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("no device named {name}"))
    }

    #[test]
    fn classify_table_gives_the_expected_kind_and_transport() {
        let snapshot = parse_system_profiler(FIXTURE).unwrap();
        assert_eq!(snapshot.devices.len(), 11);
        assert_eq!(snapshot.skipped, 0);
        let expected = [
            (
                "Built-in Microphone",
                DeviceKind::Microphone,
                Transport::BuiltIn,
                1,
                0,
            ),
            (
                "Built-in Speakers",
                DeviceKind::BuiltInSpeakers,
                Transport::BuiltIn,
                0,
                2,
            ),
            (
                "Built-in Headphones",
                DeviceKind::Headphones,
                Transport::BuiltIn,
                0,
                2,
            ),
            (
                "Virtual Device 2ch",
                DeviceKind::VirtualInput2ch,
                Transport::Virtual,
                2,
                2,
            ),
            (
                "Virtual Device 16ch",
                DeviceKind::VirtualOutput16ch,
                Transport::Virtual,
                16,
                16,
            ),
            (
                "Wireless Headset",
                DeviceKind::Headphones,
                Transport::Bluetooth,
                1,
                2,
            ),
            ("USB Headset", DeviceKind::Headphones, Transport::Usb, 1, 2),
            ("Display Audio", DeviceKind::Other, Transport::Display, 0, 2),
            (
                "Virtual Device odd",
                DeviceKind::Other,
                Transport::Virtual,
                2,
                0,
            ),
            ("Silent Port", DeviceKind::Other, Transport::BuiltIn, 0, 0),
            (
                "Unlabelled Input",
                DeviceKind::Microphone,
                Transport::Other,
                1,
                0,
            ),
        ];
        for (name, kind, transport, input, output) in expected {
            let d = by_name(&snapshot, name);
            assert_eq!(d.kind, kind, "kind of {name}");
            assert_eq!(d.transport, transport, "transport of {name}");
            assert_eq!(
                (d.input_channels, d.output_channels),
                (input, output),
                "channels of {name}"
            );
            assert_eq!(d.id, name, "id of {name}");
        }
    }

    #[test]
    fn default_flags_come_from_the_tool_output() {
        let snapshot = parse_system_profiler(FIXTURE).unwrap();
        assert!(by_name(&snapshot, "Built-in Microphone").is_default_input);
        assert!(by_name(&snapshot, "Built-in Headphones").is_default_output);
        let defaults = snapshot
            .devices
            .iter()
            .filter(|d| d.is_default_input || d.is_default_output)
            .count();
        assert_eq!(defaults, 2);
    }

    #[test]
    fn empty_items_gives_ok_empty_list() {
        let text = r#"{"SPAudioDataType":[{"_items":[],"_name":"coreaudio_device"}]}"#;
        assert_eq!(parse_system_profiler(text), Ok(DeviceSnapshot::default()));
    }

    #[test]
    fn invalid_json_gives_invalid_output() {
        assert_eq!(
            parse_system_profiler("not json"),
            Err(DeviceListError::InvalidOutput)
        );
        assert_eq!(
            parse_system_profiler(""),
            Err(DeviceListError::InvalidOutput)
        );
    }

    #[test]
    fn json_without_the_audio_array_gives_invalid_output() {
        for text in [
            "{}",
            "[]",
            r#"{"SPAudioDataType":"x"}"#,
            r#"{"SPAudioDataType":null}"#,
        ] {
            assert_eq!(
                parse_system_profiler(text),
                Err(DeviceListError::InvalidOutput),
                "{text}"
            );
        }
    }

    #[test]
    fn item_without_name_is_skipped_and_others_kept() {
        let text = r#"{"SPAudioDataType":[{"_items":[
            {"coreaudio_device_input":1},
            {"_name":"Kept","coreaudio_device_input":1,"coreaudio_device_transport":"coreaudio_device_type_builtin"}
        ]}]}"#;
        let snapshot = parse_system_profiler(text).unwrap();
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].name, "Kept");
        assert_eq!(snapshot.skipped, 1);
    }

    #[test]
    fn unknown_transport_string_is_kept_as_other() {
        let text = r#"{"SPAudioDataType":[{"_items":[
            {"_name":"Odd","coreaudio_device_output":2,"coreaudio_device_transport":"coreaudio_device_type_future"}
        ]}]}"#;
        let snapshot = parse_system_profiler(text).unwrap();
        assert_eq!(snapshot.devices[0].transport, Transport::Other);
        assert_eq!(snapshot.devices[0].kind, DeviceKind::Other);
    }

    #[test]
    fn channel_values_that_are_not_numbers_count_as_zero() {
        let text = r#"{"SPAudioDataType":[{"_items":[
            {"_name":"Bad","coreaudio_device_input":"two","coreaudio_device_output":-1}
        ]}]}"#;
        let d = &parse_system_profiler(text).unwrap().devices[0];
        assert_eq!((d.input_channels, d.output_channels), (0, 0));
    }

    #[test]
    fn headphone_word_is_matched_without_case() {
        assert_eq!(
            classify("EXTERNAL HEADPHONES", 0, 2, Transport::BuiltIn),
            DeviceKind::Headphones
        );
        assert_eq!(
            classify("Lautsprecher", 0, 2, Transport::BuiltIn),
            DeviceKind::BuiltInSpeakers
        );
    }

    struct FakeRunner {
        result: Result<Vec<u8>, DeviceListError>,
        seen: RefCell<Vec<(String, Vec<String>, Duration)>>,
    }

    impl FakeRunner {
        fn new(result: Result<Vec<u8>, DeviceListError>) -> Self {
            Self {
                result,
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl ToolRunner for FakeRunner {
        fn run(
            &self,
            program: &str,
            args: &[&str],
            limit: Duration,
        ) -> Result<Vec<u8>, DeviceListError> {
            self.seen.borrow_mut().push((
                program.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
                limit,
            ));
            self.result.clone()
        }
    }

    #[test]
    fn reader_runs_only_the_fixed_tool_with_its_limit() {
        let runner = FakeRunner::new(Ok(FIXTURE.as_bytes().to_vec()));
        let snapshot = read_system_devices(&runner).unwrap();
        assert_eq!(snapshot.devices.len(), 11);
        let seen = runner.seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "system_profiler");
        assert_eq!(seen[0].1, ["SPAudioDataType", "-json"]);
        assert_eq!(seen[0].2, Duration::from_millis(SYSTEM_PROFILER_TIMEOUT_MS));
    }

    #[test]
    fn tool_not_found_gives_tool_not_found() {
        let runner = FakeRunner::new(Err(DeviceListError::ToolNotFound));
        assert_eq!(
            read_system_devices(&runner),
            Err(DeviceListError::ToolNotFound)
        );
    }

    #[test]
    fn non_zero_exit_gives_tool_failed() {
        let runner = FakeRunner::new(Err(DeviceListError::ToolFailed));
        assert_eq!(
            read_system_devices(&runner),
            Err(DeviceListError::ToolFailed)
        );
    }

    #[test]
    fn non_utf8_output_gives_invalid_output() {
        let runner = FakeRunner::new(Ok(vec![0xff, 0xfe, 0xfd]));
        assert_eq!(
            read_system_devices(&runner),
            Err(DeviceListError::InvalidOutput)
        );
    }

    #[test]
    fn slow_tool_times_out_within_two_seconds() {
        let started = Instant::now();
        let result = ProcessRunner.run("/bin/sleep", &["5"], Duration::from_millis(100));
        assert_eq!(result, Err(DeviceListError::TimedOut));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn spawn_error_kinds_map_to_tool_errors() {
        use std::io::{Error, ErrorKind};
        assert_eq!(
            spawn_error(&Error::from(ErrorKind::NotFound)),
            DeviceListError::ToolNotFound
        );
        assert_eq!(
            spawn_error(&Error::from(ErrorKind::PermissionDenied)),
            DeviceListError::ToolFailed
        );
    }
}
