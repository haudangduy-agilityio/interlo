//! diagnostics: log events with timings, event names and error codes, and a
//! redacted export.
//!
//! Owns: the `LogEvent` shape, the well-known event names and error codes, and
//! the redaction step that runs before any event is exported.
//!
//! Must never: write transcript text unless debug logs are switched on.
//! `export_json_lines` is the only way to produce output from an event and it
//! drops `text` when `debug_on` is false.

use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;

/// Well-known event names.
pub mod event {
    pub const SESSION_STARTED: &str = "session.started";
    pub const SESSION_STOPPED: &str = "session.stopped";
    pub const TRANSLATION_LATENCY: &str = "translation.latency";
    pub const EGRESS_BLOCKED: &str = "privacy.egress.blocked";
    pub const SUMMARY_JOB_FAILED: &str = "summary.job.failed";
    pub const MODEL_VERIFY_FAILED: &str = "model.verify.failed";
    /// Capture dropped samples because the consumer was too slow. The dropped
    /// audio length goes in the `dropped` timing, in milliseconds.
    pub const CAPTURE_OVERFLOW: &str = "audio.capture.overflow";
}

/// Well-known error codes.
pub mod code {
    pub const EGRESS_LOCAL_ONLY: &str = "E_EGRESS_LOCAL_ONLY";
    pub const EGRESS_NO_CONSENT: &str = "E_EGRESS_NO_CONSENT";
    /// A download host is not on the allow list of its purpose.
    pub const EGRESS_HOST_NOT_ALLOWED: &str = "E_EGRESS_HOST_NOT_ALLOWED";
    /// The url is not `https://` with a host.
    pub const EGRESS_BAD_URL: &str = "E_EGRESS_BAD_URL";
    /// A meeting session is active, so no network call is allowed.
    pub const EGRESS_MEETING_RUNNING: &str = "E_EGRESS_MEETING_RUNNING";
    /// A provider that calls itself local asked the guard for the network.
    pub const EGRESS_LOCAL_PROVIDER: &str = "E_EGRESS_LOCAL_PROVIDER";
    pub const MODEL_CHECKSUM: &str = "E_MODEL_CHECKSUM";
    pub const MODEL_SIZE_CAP: &str = "E_MODEL_SIZE_CAP";
    pub const JOB_OUT_OF_ATTEMPTS: &str = "E_JOB_OUT_OF_ATTEMPTS";
    pub const CAPTURE_RING_FULL: &str = "E_CAPTURE_RING_FULL";
}

#[derive(Debug, Error)]
pub enum DiagError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// One log event. Fields are private and `name`, `code` and timing labels are
/// `&'static str`, so only a constant can fill them; runtime text goes only
/// into `text`, which `export_json_lines` drops unless debug logs are on.
/// The type does not implement `Serialize` and its `Debug` hides `text`, so
/// `export_json_lines` is the only way to produce output from an event.
///
/// ```compile_fail
/// // Serializing an event directly would skip the redaction step.
/// let e = diagnostics::LogEvent::new(diagnostics::event::SESSION_STARTED).with_text("hello");
/// let _ = serde_json::to_string(&e);
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct LogEvent {
    name: &'static str,
    code: Option<&'static str>,
    /// Named durations in milliseconds, for example `"stt" -> 120`.
    timings_ms: BTreeMap<&'static str, u64>,
    /// Transcript or translation text. Kept only when debug logs are on.
    text: Option<String>,
}

impl fmt::Debug for LogEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogEvent")
            .field("name", &self.name)
            .field("code", &self.code)
            .field("timings_ms", &self.timings_ms)
            .field("text", &self.text.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}

impl LogEvent {
    /// `name` is a `&'static str` so a runtime string (an error message, a
    /// transcript) cannot be used as an event name.
    ///
    /// ```compile_fail
    /// let runtime = String::from("transcript text");
    /// let _ = diagnostics::LogEvent::new(&runtime);
    /// ```
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            code: None,
            timings_ms: BTreeMap::new(),
            text: None,
        }
    }

    /// ```compile_fail
    /// let runtime = String::from("E_FROM_AN_ERROR_MESSAGE");
    /// let _ = diagnostics::LogEvent::new(diagnostics::event::SESSION_STARTED).with_code(&runtime);
    /// ```
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    /// ```compile_fail
    /// let runtime = String::from("stt");
    /// let _ = diagnostics::LogEvent::new(diagnostics::event::SESSION_STARTED).with_timing(&runtime, 1);
    /// ```
    pub fn with_timing(mut self, label: &'static str, ms: u64) -> Self {
        self.timings_ms.insert(label, ms);
        self
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn code(&self) -> Option<&'static str> {
        self.code
    }

    pub fn timing_ms(&self, label: &str) -> Option<u64> {
        self.timings_ms.get(label).copied()
    }
}

/// The JSON shape of one exported line.
#[derive(Serialize)]
struct Line<'a> {
    name: &'static str,
    code: Option<&'static str>,
    timings_ms: &'a BTreeMap<&'static str, u64>,
    text: Option<&'a str>,
}

/// One JSON object per line. Text is left out unless `debug_on` is true.
/// This is the only way to turn events into output.
pub fn export_json_lines(events: &[LogEvent], debug_on: bool) -> Result<String, DiagError> {
    let mut out = String::new();
    for e in events {
        let line = Line {
            name: e.name,
            code: e.code,
            timings_ms: &e.timings_ms,
            text: if debug_on { e.text.as_deref() } else { None },
        };
        out.push_str(&serde_json::to_string(&line)?);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LogEvent {
        LogEvent::new(event::TRANSLATION_LATENCY)
            .with_code(code::MODEL_SIZE_CAP)
            .with_timing("stt", 120)
            .with_timing("mt", 80)
            .with_text("the quarterly numbers are down")
    }

    fn lines(out: &str) -> Vec<serde_json::Value> {
        out.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn egress_codes_are_distinct_and_start_with_e_egress() {
        let codes = [
            code::EGRESS_LOCAL_ONLY,
            code::EGRESS_NO_CONSENT,
            code::EGRESS_HOST_NOT_ALLOWED,
            code::EGRESS_BAD_URL,
            code::EGRESS_MEETING_RUNNING,
            code::EGRESS_LOCAL_PROVIDER,
        ];
        for c in codes {
            assert!(c.starts_with("E_EGRESS_"), "{c}");
        }
        let distinct: std::collections::BTreeSet<_> = codes.into_iter().collect();
        assert_eq!(distinct.len(), codes.len());
    }

    #[test]
    fn debug_off_drops_the_text_and_keeps_the_rest() {
        let out = export_json_lines(&[sample()], false).unwrap();
        let v = &lines(&out)[0];
        assert_eq!(v["text"], serde_json::Value::Null);
        assert_eq!(v["name"], event::TRANSLATION_LATENCY);
        assert_eq!(v["code"], code::MODEL_SIZE_CAP);
        assert_eq!(v["timings_ms"]["stt"], 120);
        assert_eq!(v["timings_ms"]["mt"], 80);
        assert!(!out.contains("quarterly"));
    }

    #[test]
    fn debug_on_keeps_the_text() {
        let out = export_json_lines(&[sample()], true).unwrap();
        assert_eq!(lines(&out)[0]["text"], "the quarterly numbers are down");
    }

    #[test]
    fn export_carries_no_text_when_debug_is_off() {
        let out = export_json_lines(&[sample(), sample()], false).unwrap();
        assert_eq!(out.lines().count(), 2);
        assert!(!out.contains("quarterly"));
        for v in lines(&out) {
            assert_eq!(v["text"], serde_json::Value::Null);
        }
    }

    #[test]
    fn export_is_the_same_for_repeated_calls() {
        let events = [sample()];
        assert_eq!(
            export_json_lines(&events, false).unwrap(),
            export_json_lines(&events, false).unwrap()
        );
    }

    #[test]
    fn export_of_nothing_is_empty() {
        assert_eq!(export_json_lines(&[], false).unwrap(), "");
    }

    #[test]
    fn export_keeps_the_event_json_shape() {
        let e = LogEvent::new(event::SESSION_STARTED);
        assert_eq!(
            export_json_lines(&[e], false).unwrap(),
            "{\"name\":\"session.started\",\"code\":null,\"timings_ms\":{},\"text\":null}\n"
        );
    }

    #[test]
    fn accessors_return_the_constants() {
        let e = sample();
        assert_eq!(e.name(), event::TRANSLATION_LATENCY);
        assert_eq!(e.code(), Some(code::MODEL_SIZE_CAP));
        assert_eq!(e.timing_ms("stt"), Some(120));
        assert_eq!(e.timing_ms("nope"), None);
    }

    #[test]
    fn debug_output_hides_the_text() {
        let shown = format!("{:?}", sample());
        assert!(!shown.contains("quarterly"), "{shown}");
        assert!(shown.contains("translation.latency"));
    }
}
