//! Offline checks: read the log of one spike run and say whether an acceptance criterion
//! holds. The rules come from the SPEC definitions (levels in `levels.rs`); the periods and
//! limits below are its starting guesses.

use crate::levels::{audio_present, energy_mean_dbfs, within_idle, WITHIN_DB};

/// Seconds of a "period" in the checks (AC-11, AC-15, AC-19, AC-35).
pub const PERIOD_SECONDS: usize = 10;
/// One-second windows of a period that must be "audio present".
pub const PRESENT_SECONDS_OF_TEN: usize = 9;
/// A one-second bucket needs at least this many 100 ms windows to count as measured.
const MIN_WINDOWS_PER_SECOND: usize = 8;
/// AC-11: the quiet check runs from this many ms after `SOURCE_END` ...
pub const QUIET_FROM_MS: u64 = 2_000;
/// ... to this many ms after it.
pub const QUIET_TO_MS: u64 = 5_000;
/// AC-14: no single window may be more than this many dB above the idle floor.
pub const SINGLE_WINDOW_LIMIT_DB: f64 = 10.0;
/// AC-16: `SILENT` must come within this many ms of `TAP_START`.
pub const SILENT_DEADLINE_MS: u64 = 11_000;
/// AC-17, AC-20, AC-36, AC-40: the status lines must come within this many ms of `START`.
pub const STATUS_DEADLINE_MS: u64 = 2_000;
/// AC-18: `RESTART` within this many ms of `RATE_CHANGE`, and audio present within this many
/// ms after `RESTART`.
pub const RESTART_DEADLINE_MS: u64 = 3_000;
/// Seconds the self tone plays (AC-12).
pub const SELF_TONE_MS: u64 = 5_000;

/// One log line: `<ms> <event> <rest>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub ms: u64,
    pub event: String,
    pub rest: String,
}

impl Line {
    /// The value of a `key=value` token in the rest of the line.
    pub fn field(&self, key: &str) -> Option<&str> {
        self.rest
            .split_whitespace()
            .find_map(|t| t.strip_prefix(key)?.strip_prefix('='))
    }
}

/// One 100 ms window reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Win {
    pub ms: u64,
    pub level: f64,
    pub tone: f64,
}

/// A parsed spike log.
#[derive(Debug, Clone, PartialEq)]
pub struct Log {
    pub lines: Vec<Line>,
}

/// The answer of one check.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub pass: bool,
    pub detail: String,
}

impl Verdict {
    pub fn yes(detail: impl Into<String>) -> Self {
        Self {
            pass: true,
            detail: detail.into(),
        }
    }
    pub fn no(detail: impl Into<String>) -> Self {
        Self {
            pass: false,
            detail: detail.into(),
        }
    }
}

impl Log {
    pub fn parse(text: &str) -> Log {
        let lines = text
            .lines()
            .filter_map(|l| {
                let (ms, rest) = l.split_once(' ')?;
                let ms = ms.parse().ok()?;
                let (event, rest) = rest.split_once(' ').unwrap_or((rest, ""));
                Some(Line {
                    ms,
                    event: event.to_string(),
                    rest: rest.to_string(),
                })
            })
            .collect();
        Log { lines }
    }

    pub fn first(&self, event: &str) -> Option<&Line> {
        self.lines.iter().find(|l| l.event == event)
    }

    /// Time of the first `MARK <label>` line.
    pub fn mark(&self, label: &str) -> Option<u64> {
        self.lines
            .iter()
            .find(|l| l.event == "MARK" && l.rest.trim() == label)
            .map(|l| l.ms)
    }

    pub fn windows(&self) -> Vec<Win> {
        self.lines
            .iter()
            .filter(|l| l.event == "WIN")
            .filter_map(|l| {
                Some(Win {
                    ms: l.ms,
                    level: l.field("level")?.parse().ok()?,
                    tone: l.field("tone")?.parse().ok()?,
                })
            })
            .collect()
    }

    /// `(IDLE_FLOOR, IDLE_TONE)` in dBFS.
    pub fn idle(&self) -> Option<(f64, f64)> {
        let floor = self.first("IDLE_FLOOR")?.field("dbfs")?.parse().ok()?;
        let tone = self.first("IDLE_TONE")?.field("dbfs")?.parse().ok()?;
        Some((floor, tone))
    }
}

/// Energy means of the one-second buckets after `start_ms`; `None` for a bucket with too few windows.
fn second_means(wins: &[Win], start_ms: u64, seconds: usize, tone: bool) -> Vec<Option<f64>> {
    (0..seconds as u64)
        .map(|k| {
            let from = start_ms + k * 1000;
            let vals: Vec<f64> = wins
                .iter()
                .filter(|w| w.ms > from && w.ms <= from + 1000)
                .map(|w| if tone { w.tone } else { w.level })
                .collect();
            (vals.len() >= MIN_WINDOWS_PER_SECOND).then(|| energy_mean_dbfs(&vals))
        })
        .collect()
}

/// Windows with `from_ms < ms <= to_ms`.
fn between(wins: &[Win], from_ms: u64, to_ms: u64) -> Vec<Win> {
    wins.iter()
        .copied()
        .filter(|w| w.ms > from_ms && w.ms <= to_ms)
        .collect()
}

/// "Audio present over a period": 9 of 10 one-second windows from `start_ms` are present.
pub fn present_over_period(log: &Log, start_ms: u64) -> Verdict {
    let Some((floor, _)) = log.idle() else {
        return Verdict::no("no IDLE_FLOOR line");
    };
    let means = second_means(&log.windows(), start_ms, PERIOD_SECONDS, false);
    let present = means
        .iter()
        .filter(|m| m.is_some_and(|v| audio_present(v, floor)))
        .count();
    let missing = means.iter().filter(|m| m.is_none()).count();
    let readings: Vec<String> = means
        .iter()
        .map(|m| m.map_or("none".to_string(), |v| format!("{v:.1}")))
        .collect();
    let detail = format!(
        "idle_floor={floor:.1} present_seconds={present}/{PERIOD_SECONDS} unmeasured_seconds={missing} means_dbfs=[{}]",
        readings.join(", ")
    );
    if present >= PRESENT_SECONDS_OF_TEN {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// AC-11: present for 10 s from the mark, then within 3 dB of the idle floor from 2 s to 5 s
/// after `SOURCE_END`.
pub fn check_ac11(log: &Log, start_label: &str) -> Verdict {
    let Some(start) = log.mark(start_label) else {
        return Verdict::no(format!("no MARK {start_label} line"));
    };
    let first = present_over_period(log, start);
    let Some(end) = log.first("SOURCE_END").map(|l| l.ms) else {
        return Verdict::no(format!("no SOURCE_END line; {}", first.detail));
    };
    let Some((floor, _)) = log.idle() else {
        return Verdict::no("no IDLE_FLOOR line");
    };
    let quiet = between(&log.windows(), end + QUIET_FROM_MS, end + QUIET_TO_MS);
    let expected = ((QUIET_TO_MS - QUIET_FROM_MS) / 100) as usize;
    let loud = quiet
        .iter()
        .filter(|w| !within_idle(w.level, floor))
        .count();
    let max = quiet
        .iter()
        .map(|w| w.level)
        .fold(f64::NEG_INFINITY, f64::max);
    let detail = format!(
        "{}; quiet_windows={}/{expected} windows_outside_3dB={loud} max_level={max:.1}",
        first.detail,
        quiet.len()
    );
    if first.pass && loud == 0 && quiet.len() + 5 >= expected {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// A 1 kHz tone must not reach the tap: the mean tone level over the period is within 3 dB of
/// `IDLE_TONE`.
fn tone_absent_between(log: &Log, from_ms: u64, to_ms: u64) -> Verdict {
    let Some((_, idle_tone)) = log.idle() else {
        return Verdict::no("no IDLE_TONE line");
    };
    let tones: Vec<f64> = between(&log.windows(), from_ms, to_ms)
        .iter()
        .map(|w| w.tone)
        .collect();
    let need = ((to_ms - from_ms) / 100) as usize;
    let mean = energy_mean_dbfs(&tones);
    let detail = format!(
        "idle_tone={idle_tone:.1} mean_tone={mean:.1} windows={}/{need} limit_db={WITHIN_DB}",
        tones.len()
    );
    if tones.len() + 5 >= need && within_idle(mean, idle_tone) {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// AC-12: the spike's own tone (`SELF_TONE_START` plus 5 s) is not in the tap.
pub fn check_ac12(log: &Log) -> Verdict {
    match log.first("SELF_TONE_START") {
        Some(l) => tone_absent_between(log, l.ms, l.ms + SELF_TONE_MS),
        None => Verdict::no("no SELF_TONE_START line"),
    }
}

/// AC-13: a tone played for `seconds` from the mark is not in the tap.
pub fn check_ac13(log: &Log, start_label: &str, seconds: u64) -> Verdict {
    match log.mark(start_label) {
        Some(start) => tone_absent_between(log, start, start + seconds * 1000),
        None => Verdict::no(format!("no MARK {start_label} line")),
    }
}

/// AC-14: the 10 s after the mark stay within 3 dB of the idle floor, no window above +10 dB.
pub fn check_ac14(log: &Log, start_label: &str) -> Verdict {
    let Some(start) = log.mark(start_label) else {
        return Verdict::no(format!("no MARK {start_label} line"));
    };
    let Some((floor, _)) = log.idle() else {
        return Verdict::no("no IDLE_FLOOR line");
    };
    let wins = between(&log.windows(), start, start + 10_000);
    let levels: Vec<f64> = wins.iter().map(|w| w.level).collect();
    let mean = energy_mean_dbfs(&levels);
    let max = levels.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let detail = format!(
        "idle_floor={floor:.1} mean_level={mean:.1} max_window={max:.1} windows={}/100",
        levels.len()
    );
    if levels.len() >= 95 && within_idle(mean, floor) && max <= floor + SINGLE_WINDOW_LIMIT_DB {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// AC-16: `SILENT` within 11 s of `TAP_START`.
pub fn check_ac16(log: &Log) -> Verdict {
    let (Some(start), Some(silent)) = (log.first("TAP_START"), log.first("SILENT")) else {
        return Verdict::no("no TAP_START or no SILENT line");
    };
    let delay = silent.ms.saturating_sub(start.ms);
    let detail = format!("SILENT {delay} ms after TAP_START (limit {SILENT_DEADLINE_MS} ms)");
    if silent.ms >= start.ms && delay <= SILENT_DEADLINE_MS {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// The status lines between `START` and the first capture start (or the end), in order.
fn status_lines(log: &Log) -> Vec<&Line> {
    log.lines
        .iter()
        .skip_while(|l| l.event != "START")
        .skip(1)
        .take_while(|l| !matches!(l.event.as_str(), "TAP_START" | "DEVICE_START" | "WIN"))
        .filter(|l| l.event != "PERMISSION_PROMPT")
        .collect()
}

/// AC-17, AC-20, AC-40: the expected status lines, in order, each within 2 s of `START`, and a
/// capture started.
pub fn check_route_lines(log: &Log, expected: &[&str]) -> Verdict {
    let Some(start) = log.first("START").map(|l| l.ms) else {
        return Verdict::no("no START line");
    };
    let got = status_lines(log);
    let names: Vec<&str> = got.iter().map(|l| l.event.as_str()).collect();
    let late = got
        .iter()
        .filter(|l| l.ms.saturating_sub(start) > STATUS_DEADLINE_MS)
        .count();
    let started = log
        .first("TAP_START")
        .or_else(|| log.first("DEVICE_START"))
        .is_some();
    let detail = format!(
        "lines={names:?} expected={expected:?} late_lines={late} capture_started={started}"
    );
    if names == expected && late == 0 && started {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// AC-36: the causes, then `NO_CAPTURE_ROUTE 2ch=missing` within 2 s, and no capture started.
pub fn check_no_route(log: &Log, causes: &[&str]) -> Verdict {
    let Some(start) = log.first("START").map(|l| l.ms) else {
        return Verdict::no("no START line");
    };
    let got = status_lines(log);
    let mut names: Vec<String> = got.iter().map(|l| l.event.clone()).collect();
    if let Some(last) = got.last().filter(|l| l.event == "NO_CAPTURE_ROUTE") {
        names.pop();
        names.push(format!("{} {}", last.event, last.rest.trim()));
    }
    let mut expected: Vec<String> = causes.iter().map(|c| c.to_string()).collect();
    expected.push("NO_CAPTURE_ROUTE 2ch=missing".to_string());
    let late = got
        .iter()
        .filter(|l| l.ms.saturating_sub(start) > STATUS_DEADLINE_MS)
        .count();
    let started = log
        .first("TAP_START")
        .or_else(|| log.first("DEVICE_START"))
        .is_some();
    let detail = format!(
        "lines={names:?} expected={expected:?} late_lines={late} capture_started={started}"
    );
    if names == expected && late == 0 && !started {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// AC-35 and AC-19: the route line is `want` (`route=tap` or `route=2ch`), no `NO_CAPTURE_ROUTE`,
/// and the level is present over the 10 s from the mark.
pub fn check_route_present(log: &Log, want: &str, start_label: &str) -> Verdict {
    let route = log
        .lines
        .iter()
        .find(|l| l.event.starts_with("route="))
        .map(|l| l.event.as_str());
    let refused = log.first("NO_CAPTURE_ROUTE").is_some();
    let Some(start) = log.mark(start_label) else {
        return Verdict::no(format!("no MARK {start_label} line"));
    };
    let period = present_over_period(log, start);
    let detail = format!(
        "route={route:?} want={want} no_capture_route={refused}; {}",
        period.detail
    );
    if route == Some(want) && !refused && period.pass {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// AC-18: `RESTART` within 3 s of `RATE_CHANGE`, and an audio-present one-second window ending
/// within 3 s of `RESTART`. The detail holds the two measured times.
pub fn check_ac18(log: &Log) -> Verdict {
    let (Some(change), Some(restart)) = (log.first("RATE_CHANGE"), log.first("RESTART")) else {
        return Verdict::no("no RATE_CHANGE or no RESTART line");
    };
    let Some((floor, _)) = log.idle() else {
        return Verdict::no("no IDLE_FLOOR line");
    };
    let restart_delay = restart.ms.saturating_sub(change.ms);
    let wins = log.windows();
    let means = second_means(
        &wins,
        restart.ms,
        (RESTART_DEADLINE_MS / 1000) as usize,
        false,
    );
    let first_present = means
        .iter()
        .position(|m| m.is_some_and(|v| audio_present(v, floor)));
    let detail = format!(
        "restart_after_rate_change_ms={restart_delay} first_present_window_end_ms_after_restart={} idle_floor={floor:.1}",
        first_present.map_or("none".to_string(), |k| ((k as u64 + 1) * 1000).to_string())
    );
    if restart.ms >= change.ms && restart_delay <= RESTART_DEADLINE_MS && first_present.is_some() {
        Verdict::yes(detail)
    } else {
        Verdict::no(detail)
    }
}

/// AC-39: the sampled `lsof -i` log holds no non-loopback remote address and no wildcard
/// listener. Sample markers are lines that start with `# sample`; a log without one is no answer.
pub fn check_lsof(text: &str) -> Verdict {
    let samples = text.lines().filter(|l| l.starts_with("# sample")).count();
    if samples == 0 {
        return Verdict::no("no sample lines in the lsof log");
    }
    let mut bad = Vec::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("COMMAND"))
    {
        let Some(name) = line
            .split_whitespace()
            .find(|t| t.contains(':') && !t.starts_with("0x"))
        else {
            continue;
        };
        if name.starts_with("*:") || name.starts_with("0.0.0.0:") || name.starts_with("[::]:") {
            bad.push(format!("wildcard listener: {line}"));
        } else if let Some((_, remote)) = name.split_once("->") {
            let host = remote
                .rsplit_once(':')
                .map_or(remote, |(h, _)| h)
                .trim_matches(['[', ']']);
            if !(host == "localhost" || host == "::1" || host.starts_with("127.")) {
                bad.push(format!("remote address: {line}"));
            }
        }
    }
    if bad.is_empty() {
        Verdict::yes(format!("samples={samples} network_lines=0_non_loopback"))
    } else {
        Verdict::no(format!("samples={samples} violations={}", bad.join(" | ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a log: header lines, then windows every 100 ms from `t0` with a level per window.
    struct B {
        text: String,
        t: u64,
        i: u64,
    }

    impl B {
        fn new(t0: u64) -> Self {
            B {
                text: format!("{t0} START pid=1 ignore_2ch=false\n"),
                t: t0,
                i: 0,
            }
        }
        fn line(&mut self, ms: u64, rest: &str) -> &mut Self {
            self.text.push_str(&format!("{ms} {rest}\n"));
            self
        }
        /// `n` windows of the given levels, 100 ms apart, advancing the clock.
        fn wins(&mut self, n: usize, level: f64, tone: f64) -> &mut Self {
            for _ in 0..n {
                self.t += 100;
                self.i += 1;
                self.text.push_str(&format!(
                    "{} WIN i={} level={level:.1} tone={tone:.1}\n",
                    self.t, self.i
                ));
            }
            self
        }
        fn idle(&mut self, floor: f64, tone: f64) -> &mut Self {
            self.wins(50, floor, tone);
            let t = self.t;
            self.line(t, &format!("IDLE_FLOOR dbfs={floor:.1}"));
            self.line(t, &format!("IDLE_TONE dbfs={tone:.1}"));
            self
        }
        fn log(&self) -> Log {
            Log::parse(&self.text)
        }
    }

    #[test]
    fn parse_reads_events_fields_and_marks() {
        let log = Log::parse("100 START pid=7\n150 MARK clip_start\n200 WIN i=1 level=-12.5 tone=-30.0\nnot a line\n");
        assert_eq!(log.first("START").and_then(|l| l.field("pid")), Some("7"));
        assert_eq!(log.mark("clip_start"), Some(150));
        assert_eq!(
            log.windows(),
            vec![Win {
                ms: 200,
                level: -12.5,
                tone: -30.0
            }]
        );
    }

    #[test]
    fn present_period_passes_with_nine_of_ten_loud_seconds() {
        let mut b = B::new(0);
        b.idle(-120.0, -120.0);
        let start = b.t;
        b.line(start, "MARK clip_start");
        b.wins(10, -20.0, -90.0)
            .wins(10, -120.0, -120.0)
            .wins(70, -20.0, -90.0)
            .wins(10, -20.0, -90.0);
        assert!(present_over_period(&b.log(), start).pass);
    }

    #[test]
    fn present_period_fails_with_eight_loud_seconds() {
        let mut b = B::new(0);
        b.idle(-120.0, -120.0);
        let start = b.t;
        b.wins(30, -20.0, -90.0)
            .wins(20, -120.0, -120.0)
            .wins(50, -20.0, -90.0);
        let v = present_over_period(&b.log(), start);
        assert!(!v.pass, "{}", v.detail);
    }

    #[test]
    fn present_period_fails_when_nothing_was_captured_control() {
        let mut b = B::new(0);
        b.idle(-120.0, -120.0);
        let start = b.t;
        b.wins(100, -120.0, -120.0);
        assert!(!present_over_period(&b.log(), start).pass);
    }

    #[test]
    fn present_period_does_not_count_a_gap_as_audio() {
        let mut b = B::new(0);
        b.idle(-120.0, -120.0);
        let start = b.t;
        b.wins(30, -20.0, -90.0);
        b.t += 5_000;
        b.wins(30, -20.0, -90.0);
        let v = present_over_period(&b.log(), start);
        assert!(!v.pass, "{}", v.detail);
    }

    fn ac11_log(tail_level: f64) -> B {
        let mut b = B::new(0);
        b.idle(-120.0, -120.0);
        let start = b.t;
        b.line(start, "MARK clip_start");
        b.wins(100, -20.0, -90.0);
        let end = b.t;
        b.line(end + 50, "SOURCE_END");
        b.wins(20, -120.0, -120.0)
            .wins(30, tail_level, -120.0)
            .wins(10, -120.0, -120.0);
        b
    }

    #[test]
    fn ac11_passes_when_audio_present_then_quiet_after_source_end() {
        let v = check_ac11(&ac11_log(-120.0).log(), "clip_start");
        assert!(v.pass, "{}", v.detail);
    }

    #[test]
    fn ac11_fails_when_the_tap_still_hears_something_after_the_source_ended() {
        let v = check_ac11(&ac11_log(-60.0).log(), "clip_start");
        assert!(!v.pass, "{}", v.detail);
    }

    #[test]
    fn ac11_fails_without_source_end() {
        let mut b = B::new(0);
        b.idle(-120.0, -120.0);
        let s = b.t;
        b.line(s, "MARK clip_start").wins(100, -20.0, -90.0);
        assert!(!check_ac11(&b.log(), "clip_start").pass);
    }

    #[test]
    fn tone_checks_pass_when_tone_stays_at_idle_and_fail_when_it_leaks() {
        for (tone, want) in [(-120.0, true), (-30.0, false)] {
            let mut b = B::new(0);
            b.idle(-120.0, -120.0);
            let s = b.t;
            b.line(s, "MARK tone_start").line(s, "SELF_TONE_START");
            b.wins(100, -120.0, tone);
            assert_eq!(
                check_ac13(&b.log(), "tone_start", 10).pass,
                want,
                "tone {tone}"
            );
            assert_eq!(check_ac12(&b.log()).pass, want, "tone {tone}");
        }
    }

    #[test]
    fn tone_check_is_not_fooled_by_a_short_loud_burst() {
        let mut b = B::new(0);
        b.idle(-120.0, -120.0);
        let s = b.t;
        b.line(s, "MARK tone_start");
        b.wins(2, -20.0, -20.0).wins(98, -120.0, -120.0);
        assert!(!check_ac13(&b.log(), "tone_start", 10).pass);
    }

    #[test]
    fn ac14_limits_the_mean_and_every_single_window() {
        let build = |burst: f64| {
            let mut b = B::new(0);
            b.idle(-120.0, -120.0);
            let s = b.t;
            b.line(s, "MARK notify");
            b.wins(50, -120.0, -120.0)
                .wins(1, burst, -120.0)
                .wins(49, -120.0, -120.0);
            b
        };
        assert!(check_ac14(&build(-120.0).log(), "notify").pass);
        assert!(!check_ac14(&build(-100.0).log(), "notify").pass);
    }

    #[test]
    fn ac16_silent_within_eleven_seconds() {
        let mut b = B::new(1_000);
        b.line(1_000, "TAP_START rate=48000");
        b.line(11_000, "SILENT");
        assert!(check_ac16(&b.log()).pass);
        let mut b = B::new(1_000);
        b.line(1_000, "TAP_START rate=48000");
        b.line(12_001, "SILENT");
        assert!(!check_ac16(&b.log()).pass);
        let mut b = B::new(1_000);
        b.line(1_000, "TAP_START rate=48000");
        assert!(!check_ac16(&b.log()).pass);
    }

    #[test]
    fn route_lines_must_match_in_order_and_time() {
        let mut b = B::new(0);
        b.line(100, "PERMISSION_REFUSED")
            .line(110, "route=2ch")
            .line(500, "DEVICE_START rate=48000");
        assert!(check_route_lines(&b.log(), &["PERMISSION_REFUSED", "route=2ch"]).pass);
        assert!(!check_route_lines(&b.log(), &["route=2ch"]).pass);
        let mut late = B::new(0);
        late.line(2_500, "PERMISSION_REFUSED")
            .line(2_510, "route=2ch")
            .line(2_600, "DEVICE_START rate=48000");
        assert!(!check_route_lines(&late.log(), &["PERMISSION_REFUSED", "route=2ch"]).pass);
    }

    #[test]
    fn route_lines_ignore_the_permission_prompt_line() {
        let mut b = B::new(0);
        b.line(50, "PERMISSION_PROMPT waiting for the system dialog")
            .line(100, "app=missing")
            .line(110, "route=2ch");
        b.line(300, "DEVICE_START rate=48000");
        assert!(check_route_lines(&b.log(), &["app=missing", "route=2ch"]).pass);
    }

    #[test]
    fn no_route_needs_the_causes_the_refusal_and_no_capture() {
        let mut b = B::new(0);
        b.line(100, "PERMISSION_REFUSED")
            .line(110, "app=missing")
            .line(120, "NO_CAPTURE_ROUTE 2ch=missing");
        assert!(check_no_route(&b.log(), &["PERMISSION_REFUSED", "app=missing"]).pass);
        b.line(500, "TAP_START rate=48000");
        assert!(!check_no_route(&b.log(), &["PERMISSION_REFUSED", "app=missing"]).pass);
    }

    #[test]
    fn route_present_checks_the_route_name_and_the_level() {
        let mut b = B::new(0);
        b.line(10, "route=tap");
        b.idle(-120.0, -120.0);
        let s = b.t;
        b.line(s, "MARK clip_start").wins(100, -20.0, -90.0);
        assert!(check_route_present(&b.log(), "route=tap", "clip_start").pass);
        assert!(!check_route_present(&b.log(), "route=2ch", "clip_start").pass);
        b.line(s + 5, "NO_CAPTURE_ROUTE 2ch=missing");
        assert!(!check_route_present(&b.log(), "route=tap", "clip_start").pass);
    }

    #[test]
    fn ac18_needs_restart_and_audio_present_soon_after() {
        let build = |restart_gap: u64, level: f64| {
            let mut b = B::new(0);
            b.idle(-120.0, -120.0);
            let t = b.t;
            b.line(t + 10, "RATE_CHANGE from=48000 to=44100");
            b.line(t + 10 + restart_gap, "RESTART rate=44100 took_ms=90");
            b.t = t + 10 + restart_gap;
            b.wins(40, level, -90.0);
            b
        };
        assert!(check_ac18(&build(100, -20.0).log()).pass);
        assert!(!check_ac18(&build(3_500, -20.0).log()).pass);
        assert!(!check_ac18(&build(100, -120.0).log()).pass);
        assert!(!check_ac18(&B::new(0).log()).pass);
    }

    #[test]
    fn lsof_log_passes_with_loopback_only_lines() {
        let text = "# sample 1\nCOMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\n# sample 2\n";
        assert!(check_lsof(text).pass);
        let loop_only = "# sample 1\nm0b05 1 u 4u IPv4 0x1 0t0 TCP 127.0.0.1:5000->127.0.0.1:6000 (ESTABLISHED)\n";
        assert!(check_lsof(loop_only).pass);
    }

    #[test]
    fn lsof_log_fails_on_remote_address_wildcard_listener_or_no_samples() {
        let remote = "# sample 1\nm0b05 1 u 4u IPv4 0x1 0t0 TCP 10.0.0.5:5000->93.184.216.34:443 (ESTABLISHED)\n";
        assert!(!check_lsof(remote).pass);
        let wildcard = "# sample 1\nm0b05 1 u 4u IPv4 0x1 0t0 TCP *:8080 (LISTEN)\n";
        assert!(!check_lsof(wildcard).pass);
        let any = "# sample 1\nm0b05 1 u 4u IPv4 0x1 0t0 UDP 0.0.0.0:5353\n";
        assert!(!check_lsof(any).pass);
        assert!(!check_lsof("COMMAND PID\n").pass);
    }
}
