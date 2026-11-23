//! Tap monitor: idle floor, idle tone and the Silent rule from a stream of windows.

use crate::levels::{median, within_idle, WindowLevels};

/// Windows in the idle-floor period: 5 s of 100 ms windows.
pub const IDLE_WINDOWS: usize = 50;
/// Consecutive quiet windows for the Silent rule: 5 s of 100 ms windows.
pub const SILENT_WINDOWS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MonitorEvent {
    /// Median levels of the first 5 s.
    IdleFloor { floor_dbfs: f64, tone_dbfs: f64 },
    /// The level stayed within 3 dB of the idle floor for 5 s after the idle floor was set.
    Silent,
}

#[derive(Debug, Default)]
pub struct Monitor {
    idle_rms: Vec<f64>,
    idle_tone: Vec<f64>,
    idle: Option<(f64, f64)>,
    quiet_run: usize,
    silent_sent: bool,
}

impl Monitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// The idle floor and idle tone, once measured.
    #[cfg(test)]
    pub fn idle(&self) -> Option<(f64, f64)> {
        self.idle
    }

    pub fn push(&mut self, window: WindowLevels) -> Vec<MonitorEvent> {
        let mut events = Vec::new();
        let Some((floor, _)) = self.idle else {
            self.idle_rms.push(window.rms_dbfs);
            self.idle_tone.push(window.tone_dbfs);
            if self.idle_rms.len() == IDLE_WINDOWS {
                let floor_dbfs = median(&self.idle_rms).unwrap_or(crate::levels::FLOOR_DBFS);
                let tone_dbfs = median(&self.idle_tone).unwrap_or(crate::levels::FLOOR_DBFS);
                self.idle = Some((floor_dbfs, tone_dbfs));
                events.push(MonitorEvent::IdleFloor {
                    floor_dbfs,
                    tone_dbfs,
                });
            }
            return events;
        };
        if within_idle(window.rms_dbfs, floor) {
            self.quiet_run += 1;
        } else {
            self.quiet_run = 0;
        }
        if self.quiet_run >= SILENT_WINDOWS && !self.silent_sent {
            self.silent_sent = true;
            events.push(MonitorEvent::Silent);
        }
        events
    }
}

/// A capture whose source stopped its output stream delivers no callbacks at all. That is
/// silence, so the monitor is fed one silent window per 100 ms while no callbacks arrive.
#[derive(Debug)]
pub struct SilenceFill {
    last_window_ms: u64,
    last_callbacks: u64,
    last_callback_change_ms: u64,
}

/// No callback for this long means the stream is not running (ms, starting value).
pub const NO_CALLBACK_MS: u64 = 150;

impl SilenceFill {
    pub fn new(now_ms: u64) -> Self {
        Self {
            last_window_ms: now_ms,
            last_callbacks: 0,
            last_callback_change_ms: now_ms,
        }
    }

    /// A real window arrived at this time.
    pub fn on_window(&mut self, now_ms: u64) {
        self.last_window_ms = now_ms;
    }

    /// True when one silent window should be fed now.
    pub fn poll(&mut self, now_ms: u64, callbacks: u64) -> bool {
        if callbacks != self.last_callbacks {
            self.last_callbacks = callbacks;
            self.last_callback_change_ms = now_ms;
            return false;
        }
        let window_ms = u64::from(crate::levels::WINDOW_MS);
        if now_ms.saturating_sub(self.last_callback_change_ms) < NO_CALLBACK_MS
            || now_ms.saturating_sub(self.last_window_ms) < window_ms
        {
            return false;
        }
        // After a long gap start again from now, so that one gap gives one window, not a burst.
        self.last_window_ms = if now_ms - self.last_window_ms >= 2 * window_ms {
            now_ms
        } else {
            self.last_window_ms + window_ms
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(rms: f64, tone: f64) -> WindowLevels {
        WindowLevels {
            rms_dbfs: rms,
            tone_dbfs: tone,
        }
    }

    fn feed(m: &mut Monitor, n: usize, rms: f64, tone: f64) -> Vec<MonitorEvent> {
        (0..n).flat_map(|_| m.push(w(rms, tone))).collect()
    }

    #[test]
    fn idle_floor_is_the_median_of_the_first_five_seconds() {
        let mut m = Monitor::new();
        assert!(feed(&mut m, 49, -60.0, -70.0).is_empty());
        let ev = m.push(w(-60.0, -70.0));
        assert_eq!(
            ev,
            vec![MonitorEvent::IdleFloor {
                floor_dbfs: -60.0,
                tone_dbfs: -70.0
            }]
        );
        assert_eq!(m.idle(), Some((-60.0, -70.0)));
    }

    #[test]
    fn idle_floor_ignores_a_short_blip() {
        let mut m = Monitor::new();
        feed(&mut m, 5, -20.0, -20.0);
        let ev = feed(&mut m, 45, -60.0, -70.0);
        assert_eq!(
            ev,
            vec![MonitorEvent::IdleFloor {
                floor_dbfs: -60.0,
                tone_dbfs: -70.0
            }]
        );
    }

    #[test]
    fn silent_is_reported_after_five_more_quiet_seconds_ac16() {
        let mut m = Monitor::new();
        feed(&mut m, 50, -120.0, -120.0);
        assert!(feed(&mut m, 49, -120.0, -120.0).is_empty());
        assert_eq!(m.push(w(-120.0, -120.0)), vec![MonitorEvent::Silent]);
    }

    #[test]
    fn silent_is_reported_once() {
        let mut m = Monitor::new();
        let first = feed(&mut m, 100, -120.0, -120.0);
        let silents = first.iter().filter(|e| **e == MonitorEvent::Silent).count();
        assert_eq!(silents, 1);
        assert!(feed(&mut m, 100, -120.0, -120.0).is_empty());
    }

    #[test]
    fn a_loud_window_restarts_the_silent_count() {
        let mut m = Monitor::new();
        feed(&mut m, 50, -90.0, -90.0);
        feed(&mut m, 30, -90.0, -90.0);
        m.push(w(-40.0, -40.0));
        assert!(feed(&mut m, 49, -90.0, -90.0).is_empty());
        assert_eq!(m.push(w(-90.0, -90.0)), vec![MonitorEvent::Silent]);
    }

    #[test]
    fn window_exactly_three_db_above_floor_counts_as_quiet() {
        let mut m = Monitor::new();
        feed(&mut m, 50, -90.0, -90.0);
        feed(&mut m, 49, -87.0, -90.0);
        assert_eq!(m.push(w(-87.0, -90.0)), vec![MonitorEvent::Silent]);
    }

    #[test]
    fn music_playing_never_reports_silent() {
        let mut m = Monitor::new();
        feed(&mut m, 50, -90.0, -90.0);
        assert!(feed(&mut m, 200, -30.0, -30.0).is_empty());
    }

    #[test]
    fn no_callbacks_feed_one_silent_window_per_hundred_ms() {
        let mut f = SilenceFill::new(1_000);
        assert!(!f.poll(1_100, 0), "callback silence is not long enough yet");
        assert!(f.poll(1_200, 0));
        assert!(!f.poll(1_250, 0), "one window per 100 ms");
        assert!(f.poll(1_300, 0));
    }

    #[test]
    fn running_callbacks_never_feed_silence() {
        let mut f = SilenceFill::new(0);
        for step in 1..=50u64 {
            assert!(!f.poll(step * 20, step * 2), "callbacks advance every poll");
        }
    }

    #[test]
    fn real_windows_hold_the_fill_back_and_callbacks_resuming_stop_it() {
        let mut f = SilenceFill::new(0);
        assert!(f.poll(300, 0));
        assert!(f.poll(400, 0));
        assert!(!f.poll(420, 5), "a callback arrived");
    }

    #[test]
    fn a_real_window_alone_holds_the_fill_back() {
        let mut f = SilenceFill::new(0);
        f.on_window(950);
        assert!(!f.poll(1_000, 0), "a real window arrived 50 ms ago");
        assert!(f.poll(1_050, 0), "100 ms after the real window");
    }

    #[test]
    fn a_long_gap_does_not_burst_silent_windows() {
        let mut f = SilenceFill::new(0);
        assert!(f.poll(5_000, 0));
        assert!(!f.poll(5_010, 0));
    }
}
