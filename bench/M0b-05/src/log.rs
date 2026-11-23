//! Log lines: `<epoch_ms> <event...>` on stdout. The first token after the time is the
//! event name; the analyzer reads these lines back. Never called from a real-time callback.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Format one log line without printing it.
pub fn format_line(ms: u64, event: &str) -> String {
    format!("{ms} {event}")
}

/// Print one log line and flush, so a reader sees it at once.
pub fn emit(event: &str) {
    let line = format_line(epoch_ms(), event);
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_is_time_then_event() {
        assert_eq!(
            format_line(1763866000123, "IDLE_FLOOR dbfs=-120.0"),
            "1763866000123 IDLE_FLOOR dbfs=-120.0"
        );
    }
}
