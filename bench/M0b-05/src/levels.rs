//! Level maths for the spike: RMS and 1 kHz tone level over 100 ms windows,
//! medians, energy means and the "audio present" rule (SPEC M0b-05 definitions).
//! Pure functions and a fixed-size accumulator: nothing here allocates per sample,
//! so the accumulator may run inside the real-time capture callback.

/// Window length (starting value from the SPEC; changed only with a note).
pub const WINDOW_MS: u32 = 100;
/// Digital silence and the lower clamp for every level, in dBFS.
pub const FLOOR_DBFS: f64 = -120.0;
/// Frequency of the tone filter, in Hz.
pub const TONE_HZ: f64 = 1000.0;
/// "Audio present": a one-second mean at least this far above the idle floor (dB).
pub const PRESENT_ABOVE_FLOOR_DB: f64 = 20.0;
/// "Within" the idle floor: at most this many dB away (dB).
pub const WITHIN_DB: f64 = 3.0;

/// Levels of one 100 ms window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowLevels {
    /// RMS level of the mono mix, clamped at `FLOOR_DBFS`.
    pub rms_dbfs: f64,
    /// Level of the 1 kHz Goertzel filter, clamped at `FLOOR_DBFS`.
    pub tone_dbfs: f64,
}

/// Mean-square power to dBFS, clamped at `FLOOR_DBFS`.
pub fn power_to_dbfs(power: f64) -> f64 {
    if power <= 0.0 || !power.is_finite() {
        return FLOOR_DBFS;
    }
    (10.0 * power.log10()).max(FLOOR_DBFS)
}

/// Frames in one window at this sample rate.
pub fn window_len(sample_rate_hz: u32) -> usize {
    (u64::from(sample_rate_hz) * u64::from(WINDOW_MS) / 1000) as usize
}

/// Collects mono samples and emits `WindowLevels` every `window_len` samples.
#[derive(Debug, Clone)]
pub struct WindowAccumulator {
    window_len: usize,
    count: usize,
    sum_squares: f64,
    coeff: f64,
    s1: f64,
    s2: f64,
}

impl WindowAccumulator {
    /// A rate of zero gives a one-sample window so the type never divides by zero.
    pub fn new(sample_rate_hz: u32) -> Self {
        let rate = f64::from(sample_rate_hz.max(1));
        let omega = 2.0 * std::f64::consts::PI * TONE_HZ / rate;
        Self {
            window_len: window_len(sample_rate_hz).max(1),
            count: 0,
            sum_squares: 0.0,
            coeff: 2.0 * omega.cos(),
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Add one mono sample. Returns the levels when this sample closes a window.
    pub fn push(&mut self, sample: f32) -> Option<WindowLevels> {
        let x = f64::from(sample);
        self.sum_squares += x * x;
        let s0 = x + self.coeff * self.s1 - self.s2;
        self.s2 = self.s1;
        self.s1 = s0;
        self.count += 1;
        if self.count < self.window_len {
            return None;
        }
        let n = self.window_len as f64;
        let rms_power = self.sum_squares / n;
        let tone_energy = self.s1 * self.s1 + self.s2 * self.s2 - self.coeff * self.s1 * self.s2;
        // A sine of peak A gives |X| = A*N/2, so A^2/2 = 2|X|^2/N^2 equals its mean-square power.
        let tone_power = 2.0 * tone_energy.max(0.0) / (n * n);
        self.count = 0;
        self.sum_squares = 0.0;
        self.s1 = 0.0;
        self.s2 = 0.0;
        Some(WindowLevels {
            rms_dbfs: power_to_dbfs(rms_power),
            tone_dbfs: power_to_dbfs(tone_power),
        })
    }
}

/// Median of the values; the mean of the two middle values for an even count.
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[mid])
    } else {
        Some((sorted[mid - 1] + sorted[mid]) / 2.0)
    }
}

/// Energy mean: mean of the linear power, converted back to dBFS.
/// An empty list gives `FLOOR_DBFS`.
pub fn energy_mean_dbfs(levels_dbfs: &[f64]) -> f64 {
    if levels_dbfs.is_empty() {
        return FLOOR_DBFS;
    }
    let sum: f64 = levels_dbfs.iter().map(|l| 10f64.powf(l / 10.0)).sum();
    power_to_dbfs(sum / levels_dbfs.len() as f64)
}

/// "Audio present": a one-second mean at least 20 dB above the idle floor.
pub fn audio_present(mean_dbfs: f64, idle_floor_dbfs: f64) -> bool {
    mean_dbfs >= idle_floor_dbfs + PRESENT_ABOVE_FLOOR_DB
}

/// "Within 3 dB of" a reference level.
pub fn within_idle(level_dbfs: f64, idle_dbfs: f64) -> bool {
    (level_dbfs - idle_dbfs).abs() <= WITHIN_DB
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f64, amp: f64, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                (amp * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin()) as f32
            })
            .collect()
    }

    fn run(rate: u32, samples: &[f32]) -> Vec<WindowLevels> {
        let mut acc = WindowAccumulator::new(rate);
        samples.iter().filter_map(|s| acc.push(*s)).collect()
    }

    #[test]
    fn window_len_is_one_tenth_of_a_second() {
        assert_eq!(window_len(48_000), 4_800);
        assert_eq!(window_len(44_100), 4_410);
    }

    #[test]
    fn digital_silence_is_clamped_to_the_floor() {
        let out = run(48_000, &vec![0.0; 4_800]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rms_dbfs, FLOOR_DBFS);
        assert_eq!(out[0].tone_dbfs, FLOOR_DBFS);
    }

    #[test]
    fn tiny_signal_below_the_floor_is_clamped() {
        assert_eq!(power_to_dbfs(1e-20), FLOOR_DBFS);
        assert_eq!(power_to_dbfs(f64::NAN), FLOOR_DBFS);
        assert_eq!(power_to_dbfs(-1.0), FLOOR_DBFS);
    }

    #[test]
    fn full_scale_square_wave_is_zero_dbfs() {
        let samples: Vec<f32> = (0..4_800)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let out = run(48_000, &samples);
        assert!((out[0].rms_dbfs - 0.0).abs() < 1e-9);
    }

    #[test]
    fn one_khz_sine_reads_the_same_on_rms_and_tone_filter() {
        for rate in [48_000, 44_100] {
            let out = run(rate, &sine(rate, 1000.0, 0.5, window_len(rate)));
            let expected = 20.0 * (0.5f64 / 2f64.sqrt()).log10();
            assert!((out[0].rms_dbfs - expected).abs() < 0.01, "rms at {rate}");
            assert!((out[0].tone_dbfs - expected).abs() < 0.01, "tone at {rate}");
        }
    }

    #[test]
    fn other_frequency_is_rejected_by_the_tone_filter() {
        let out = run(48_000, &sine(48_000, 440.0, 0.5, 4_800));
        assert!(out[0].rms_dbfs > -10.0);
        assert!(out[0].tone_dbfs < out[0].rms_dbfs - 40.0);
    }

    #[test]
    fn accumulator_emits_one_window_per_window_len_samples() {
        let out = run(48_000, &vec![0.1; 4_800 * 3 + 100]);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn median_of_odd_and_even_counts() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn energy_mean_is_not_averaged_away_by_quiet_windows() {
        let mean = energy_mean_dbfs(&[0.0, -120.0, -120.0, -120.0]);
        assert!((mean - (-6.02)).abs() < 0.01, "got {mean}");
        assert!(mean > -10.0);
    }

    #[test]
    fn energy_mean_of_nothing_is_the_floor() {
        assert_eq!(energy_mean_dbfs(&[]), FLOOR_DBFS);
    }

    #[test]
    fn audio_present_boundary_is_twenty_db_above_the_floor() {
        assert!(audio_present(-100.0, -120.0));
        assert!(!audio_present(-100.01, -120.0));
        assert!(audio_present(-30.0, -60.0));
    }

    #[test]
    fn within_idle_boundary_is_three_db() {
        assert!(within_idle(-57.0, -60.0));
        assert!(within_idle(-63.0, -60.0));
        assert!(!within_idle(-56.99, -60.0));
        assert!(!within_idle(-63.01, -60.0));
    }
}
