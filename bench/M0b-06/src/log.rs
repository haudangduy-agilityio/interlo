//! One line per event: `<epoch ms> <event> [details]`. Nothing but window geometry, focus identity
//! and counts goes into a log: no audio, no screen content (guardrails 2 and 6).

/// `<ts> <event>` with the event text as it is.
pub fn format_line(ts_ms: u128, event: &str) -> String {
    format!("{ts_ms} {event}")
}

/// Splits a log line into the timestamp, the event name and the rest of the line.
pub fn parse_line(line: &str) -> Option<(u128, &str, &str)> {
    let (ts, after) = line.trim().split_once(' ')?;
    let ts = ts.parse().ok()?;
    let after = after.trim_start();
    match after.split_once(' ') {
        Some((event, rest)) => Some((ts, event, rest.trim_start())),
        None if after.is_empty() => None,
        None => Some((ts, after, "")),
    }
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Writes one event line to standard output and flushes it.
pub fn emit(event: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", format_line(now_ms(), event));
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_line_puts_the_timestamp_first() {
        assert_eq!(format_line(1700000000123, "SHOWN"), "1700000000123 SHOWN");
    }

    #[test]
    fn parse_line_splits_timestamp_event_and_rest() {
        assert_eq!(
            parse_line("1700000000123 PANEL 360,120,1200,120 display=1"),
            Some((1700000000123, "PANEL", "360,120,1200,120 display=1"))
        );
        assert_eq!(parse_line("5 SHOWN"), Some((5, "SHOWN", "")));
    }

    #[test]
    fn parse_line_refuses_a_line_without_a_numeric_timestamp_or_event() {
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("SHOWN"), None);
        assert_eq!(parse_line("12"), None);
        assert_eq!(parse_line("# sample 12"), None);
    }

    #[test]
    fn now_ms_is_after_the_year_2020() {
        assert!(now_ms() > 1_577_836_800_000);
    }
}
