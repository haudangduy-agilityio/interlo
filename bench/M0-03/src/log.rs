//! Log format v1: header lines that start with `#`, then one line per event, fields separated
//! by one tab (`<t_us>\t<stage>\t<event>\t<key>=<n>...`). Every field value is a number, so a stage cannot
//! write text into the log (guardrail 6).

use std::io::Write;

use crate::clock::Clock;

pub const FORMAT_LINE: &str = "# interlo-bench-log v1";

/// What the header says about the run. The wall-clock time is the only wall time in the log.
pub struct Header<'a> {
    pub wall_start_epoch_ms: u64,
    pub stage: &'a str,
    pub pace: &'a str,
    pub id: Option<&'a str>,
}

/// The header lines, each ended by a newline.
pub fn header_text(header: &Header) -> String {
    let mut text = format!(
        "{FORMAT_LINE}\n# wall_start_epoch_ms={}\n# stage={}\n# pace={}\n",
        header.wall_start_epoch_ms, header.stage, header.pace
    );
    if let Some(id) = header.id {
        text += &format!("# id={id}\n");
    }
    text
}

/// One event line without the final newline.
pub fn format_line(t_us: u64, stage: &str, event: &str, fields: &[(&str, u64)]) -> String {
    let mut line = format!("{t_us}\t{stage}\t{event}");
    for (key, value) in fields {
        line += &format!("\t{key}={value}");
    }
    line
}

/// Where a stage sends its events. The sink adds the time and the stage name.
pub trait EventSink {
    fn emit(&mut self, event: &'static str, fields: &[(&'static str, u64)]);
}

/// Writes the log. It reads the session clock once when it is made (time zero) and once for
/// every event line, so every time in the file comes from the one clock.
pub struct LogWriter<'a> {
    out: &'a mut dyn Write,
    clock: &'a dyn Clock,
    start: u64,
    last_us: u64,
    failed: bool,
}

impl<'a> LogWriter<'a> {
    pub fn new(out: &'a mut dyn Write, clock: &'a dyn Clock) -> Self {
        let start = clock.now_us();
        Self {
            out,
            clock,
            start,
            last_us: 0,
            failed: false,
        }
    }

    pub fn write_header(&mut self, header: &Header) {
        if self.out.write_all(header_text(header).as_bytes()).is_err() {
            self.failed = true;
        }
    }

    /// Write one event line for `stage` and return its time. One clock reading.
    pub fn emit_for(&mut self, stage: &str, event: &str, fields: &[(&str, u64)]) -> u64 {
        // The time never goes down, even if a clock reading did.
        let t_us = self.now_us().max(self.last_us);
        self.last_us = t_us;
        let line = format_line(t_us, stage, event, fields);
        if writeln!(self.out, "{line}").is_err() {
            self.failed = true;
        }
        t_us
    }

    /// Session time now, in microseconds. One clock reading, used by `realtime` pacing only.
    pub fn now_us(&self) -> u64 {
        self.clock.now_us().saturating_sub(self.start)
    }

    pub fn flush(&mut self) {
        if self.out.flush().is_err() {
            self.failed = true;
        }
    }

    /// True when a write to the log failed.
    pub fn failed(&self) -> bool {
        self.failed
    }
}

/// The sink of one stage: every event gets the stage name.
pub struct Scoped<'a, 'b> {
    pub log: &'a mut LogWriter<'b>,
    pub stage: &'a str,
}

impl EventSink for Scoped<'_, '_> {
    fn emit(&mut self, event: &'static str, fields: &[(&'static str, u64)]) {
        self.log.emit_for(self.stage, event, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_line_is_time_stage_event_fields() {
        assert_eq!(
            format_line(
                1000,
                "harness",
                "input_open",
                &[("samples", 56000), ("rate", 16000)]
            ),
            "1000\tharness\tinput_open\tsamples=56000\trate=16000"
        );
        assert_eq!(
            format_line(0, "energy_ref", "stage_start", &[]),
            "0\tenergy_ref\tstage_start"
        );
    }

    /// A clock that returns the listed readings, one per call.
    struct ScriptedClock(std::cell::RefCell<std::vec::IntoIter<u64>>);

    impl Clock for ScriptedClock {
        fn now_us(&self) -> u64 {
            self.0.borrow_mut().next().unwrap_or(0)
        }
    }

    #[test]
    fn event_time_never_goes_down_when_the_clock_does() {
        let clock = ScriptedClock(std::cell::RefCell::new(
            vec![100, 150, 140, 200].into_iter(),
        ));
        let mut out: Vec<u8> = Vec::new();
        let mut log = LogWriter::new(&mut out, &clock);
        assert_eq!(log.emit_for("harness", "a", &[]), 50);
        assert_eq!(log.emit_for("harness", "b", &[]), 50);
        assert_eq!(log.emit_for("harness", "c", &[]), 100);
    }

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn failed_write_sets_the_failed_flag() {
        let clock = crate::clock::fake::FakeClock::new(1);
        let mut out = BrokenWriter;
        let mut log = LogWriter::new(&mut out, &clock);
        assert!(!log.failed());
        log.emit_for("harness", "a", &[]);
        assert!(log.failed());
    }

    #[test]
    fn header_starts_with_version_line() {
        let text = header_text(&Header {
            wall_start_epoch_ms: 1763900000000,
            stage: "energy_ref",
            pace: "fast",
            id: None,
        });
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], FORMAT_LINE);
        assert_eq!(lines[1], "# wall_start_epoch_ms=1763900000000");
        assert_eq!(lines[2], "# stage=energy_ref");
        assert_eq!(lines[3], "# pace=fast");
        assert_eq!(lines.len(), 4);
        assert!(text.ends_with('\n'));
        let with_id = header_text(&Header {
            wall_start_epoch_ms: 1,
            stage: "energy_ref",
            pace: "realtime",
            id: Some("vi_001"),
        });
        assert!(with_id.lines().any(|l| l == "# id=vi_001"));
    }
}
