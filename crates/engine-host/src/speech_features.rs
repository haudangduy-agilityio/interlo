//! Span checks and log-mel features for the recognition model.
//!
//! The model reads 80 mel bands over a fixed window of 30 s at 16 kHz mono:
//! 3000 frames of a 400-point transform with a hop of 160 samples. The
//! extractor builds its tables once and keeps no sample after a call.

use crate::error::EngineError;
use std::f64::consts::PI;

/// Shortest span the adapter accepts: 0.1 s at 16 kHz.
pub const MIN_SPAN_SAMPLES: usize = 1_600;
/// Longest span the adapter accepts: the 30 s window of the model.
pub const MAX_SPAN_SAMPLES: usize = 480_000;
/// Mel bands of the model input.
pub(crate) const N_MELS: usize = 80;
/// Frames of the model input.
pub(crate) const FRAMES: usize = 3_000;

const SAMPLE_RATE_HZ: f64 = 16_000.0;
const N_FFT: usize = 400;
const HOP: usize = 160;
const N_BINS: usize = N_FFT / 2 + 1;
/// The model always reads a 30 s window; shorter spans are padded with zeros.
const WINDOW_SAMPLES: usize = HOP * FRAMES;
/// Power below this is treated as silence before the logarithm.
const POWER_FLOOR: f64 = 1e-10;
/// Largest drop below the loudest cell, in decades, that the model keeps.
const DYNAMIC_RANGE_LOG10: f64 = 8.0;

/// A span must hold `MIN_SPAN_SAMPLES` to `MAX_SPAN_SAMPLES` samples, all
/// finite. The text names the limits and never a sample value.
pub(crate) fn check_span(samples: &[f32]) -> Result<(), EngineError> {
    let length = samples.len();
    if !(MIN_SPAN_SAMPLES..=MAX_SPAN_SAMPLES).contains(&length) {
        return Err(EngineError::Backend(format!(
            "a span of {length} samples is outside {MIN_SPAN_SAMPLES} to {MAX_SPAN_SAMPLES} samples"
        )));
    }
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(EngineError::Backend(
            "a span holds a sample that is not a finite number".into(),
        ));
    }
    Ok(())
}

/// Slaney mel scale: linear below 1 kHz, logarithmic above.
const MEL_F_SP: f64 = 200.0 / 3.0;
const MEL_LOG_HZ: f64 = 1_000.0;

fn min_log_mel() -> f64 {
    MEL_LOG_HZ / MEL_F_SP
}

fn log_step() -> f64 {
    6.4_f64.ln() / 27.0
}

fn hz_to_mel(hz: f64) -> f64 {
    if hz >= MEL_LOG_HZ {
        min_log_mel() + (hz / MEL_LOG_HZ).ln() / log_step()
    } else {
        hz / MEL_F_SP
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    if mel >= min_log_mel() {
        MEL_LOG_HZ * (log_step() * (mel - min_log_mel())).exp()
    } else {
        MEL_F_SP * mel
    }
}

/// Triangular mel filters on a Slaney scale with Slaney area normalization,
/// 0 to 8 kHz. Layout: `N_MELS` rows of `N_BINS` weights.
fn mel_filters() -> Vec<f64> {
    let (mel_min, mel_max) = (hz_to_mel(0.0), hz_to_mel(SAMPLE_RATE_HZ / 2.0));
    let points: Vec<f64> = (0..N_MELS + 2)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f64 / (N_MELS + 1) as f64))
        .collect();
    let mut filters = vec![0.0_f64; N_MELS * N_BINS];
    for (m, row) in filters.as_chunks_mut::<N_BINS>().0.iter_mut().enumerate() {
        let (lo, mid, hi) = (points[m], points[m + 1], points[m + 2]);
        let norm = 2.0 / (hi - lo);
        for (k, weight) in row.iter_mut().enumerate() {
            let hz = (SAMPLE_RATE_HZ / 2.0) * k as f64 / (N_BINS - 1) as f64;
            let down = (hz - lo) / (mid - lo);
            let up = (hi - hz) / (hi - mid);
            *weight = down.min(up).max(0.0) * norm;
        }
    }
    filters
}

/// Log-mel features, band-major: `N_MELS` rows of `FRAMES` values. The tables
/// (about 1.5 MB) are built once by `new`.
pub(crate) struct FeatureExtractor {
    window: Vec<f64>,
    cos_table: Vec<f64>,
    sin_table: Vec<f64>,
    filters: Vec<f64>,
}

impl FeatureExtractor {
    pub(crate) fn new() -> Self {
        // Periodic Hann window.
        let window = (0..N_FFT)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / N_FFT as f64).cos())
            .collect();
        let mut cos_table = vec![0.0_f64; N_BINS * N_FFT];
        let mut sin_table = vec![0.0_f64; N_BINS * N_FFT];
        for k in 0..N_BINS {
            for i in 0..N_FFT {
                let angle = 2.0 * PI * (k * i) as f64 / N_FFT as f64;
                cos_table[k * N_FFT + i] = angle.cos();
                sin_table[k * N_FFT + i] = angle.sin();
            }
        }
        Self {
            window,
            cos_table,
            sin_table,
            filters: mel_filters(),
        }
    }

    /// Features of a span of at most `MAX_SPAN_SAMPLES` samples: zero padded to
    /// 30 s, reflected by half a window at both ends, transformed per frame,
    /// log10 clamped to 8 below the loudest cell, then `(x + 4) / 4`. The
    /// caller has run `check_span`.
    pub(crate) fn extract(&self, samples: &[f32]) -> Vec<f32> {
        let mut padded = vec![0.0_f32; WINDOW_SAMPLES];
        let used = samples.len().min(WINDOW_SAMPLES);
        padded[..used].copy_from_slice(&samples[..used]);

        // Reflect padding of half a window on both sides (centred frames).
        let pad = N_FFT / 2;
        let mut x = Vec::with_capacity(WINDOW_SAMPLES + 2 * pad);
        x.extend((1..=pad).rev().map(|i| padded[i]));
        x.extend_from_slice(&padded);
        x.extend((0..pad).map(|i| padded[WINDOW_SAMPLES - 2 - i]));

        let mut log_spec = vec![0.0_f64; N_MELS * FRAMES];
        let mut frame = vec![0.0_f64; N_FFT];
        let mut power = vec![0.0_f64; N_BINS];
        for f in 0..FRAMES {
            for ((cell, sample), weight) in frame
                .iter_mut()
                .zip(&x[f * HOP..f * HOP + N_FFT])
                .zip(&self.window)
            {
                *cell = f64::from(*sample) * weight;
            }
            for (k, bin) in power.iter_mut().enumerate() {
                let cos_row = &self.cos_table[k * N_FFT..(k + 1) * N_FFT];
                let sin_row = &self.sin_table[k * N_FFT..(k + 1) * N_FFT];
                let (mut re, mut im) = (0.0_f64, 0.0_f64);
                for ((value, c), s) in frame.iter().zip(cos_row).zip(sin_row) {
                    re += value * c;
                    im += value * s;
                }
                *bin = re * re + im * im;
            }
            for (m, row) in self.filters.as_chunks::<N_BINS>().0.iter().enumerate() {
                let energy: f64 = row.iter().zip(&power).map(|(a, b)| a * b).sum();
                log_spec[m * FRAMES + f] = energy.max(POWER_FLOOR).log10();
            }
        }
        let top = log_spec.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        log_spec
            .iter()
            .map(|&v| ((v.max(top - DYNAMIC_RANGE_LOG10) + 4.0) / 4.0) as f32)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE_HZ: f32 = 16_000.0;

    fn tone(hz: f32, phase: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / SAMPLE_RATE_HZ + phase).sin())
            .collect()
    }

    fn range(values: &[f32]) -> f32 {
        let high = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let low = values.iter().copied().fold(f32::INFINITY, f32::min);
        high - low
    }

    /// Index of the band with the largest mean over the first `frames` frames.
    fn peak_band(values: &[f32], frames: usize) -> usize {
        let mean = |band: usize| -> f32 {
            values[band * FRAMES..band * FRAMES + frames]
                .iter()
                .sum::<f32>()
                / frames as f32
        };
        let mut best = 0;
        for band in 1..N_MELS {
            if mean(band) > mean(best) {
                best = band;
            }
        }
        best
    }

    #[test]
    fn span_length_limits_are_checked_at_the_boundaries() {
        for length in [0_usize, 1_599, 480_001] {
            assert!(
                matches!(check_span(&vec![0.0; length]), Err(EngineError::Backend(_))),
                "length {length}"
            );
        }
        for length in [1_600_usize, 480_000] {
            assert_eq!(check_span(&vec![0.0; length]), Ok(()), "length {length}");
        }
    }

    #[test]
    fn span_with_a_sample_that_is_not_finite_is_refused_without_the_value() {
        let mut with_nan = vec![0.123_456_f32; 1_600];
        with_nan[800] = f32::NAN;
        let mut with_infinity = vec![0.123_456_f32; 1_600];
        with_infinity[800] = f32::INFINITY;
        for span in [with_nan, with_infinity] {
            let text = match check_span(&span) {
                Err(EngineError::Backend(text)) => text,
                other => format!("unexpected: {other:?}"),
            };
            assert!(text.contains("finite"), "{text}");
            assert!(
                !text.contains("0.123456") && !text.contains("NaN"),
                "{text}"
            );
            assert!(!text.contains("inf"), "{text}");
        }
    }

    #[test]
    fn silence_gives_the_floor_value_in_every_cell() {
        let features = FeatureExtractor::new().extract(&vec![0.0; 1_600]);
        assert_eq!(features.len(), N_MELS * FRAMES);
        assert!(
            features.iter().all(|v| (v + 1.5).abs() < 0.000_001),
            "not every cell is -1.5"
        );
    }

    #[test]
    fn tone_peaks_in_the_expected_band_and_the_range_is_two() {
        let extractor = FeatureExtractor::new();
        for (hz, band) in [(500.0_f32, 13_usize), (1_000.0, 26), (2_000.0, 44)] {
            let features = extractor.extract(&tone(hz, 0.0, 16_000));
            assert_eq!(features.len(), N_MELS * FRAMES, "{hz} Hz");
            assert_eq!(peak_band(&features, 90), band, "{hz} Hz");
            assert!((range(&features) - 2.0).abs() < 0.000_01, "{hz} Hz");
        }
    }

    #[test]
    fn output_length_is_fixed_and_the_padding_is_zeros() {
        let extractor = FeatureExtractor::new();
        // A tone that starts away from zero, so that padding with the first sample would show.
        let tone_features = extractor.extract(&tone(1_000.0, 1.0, 16_000));
        let long_features = extractor.extract(&vec![0.0; MAX_SPAN_SAMPLES]);
        assert_eq!(tone_features.len(), N_MELS * FRAMES);
        assert_eq!(long_features.len(), N_MELS * FRAMES);
        let low = tone_features.iter().copied().fold(f32::INFINITY, f32::min);
        for band in 0..N_MELS {
            let cell = tone_features[band * FRAMES + 2_000];
            assert!(
                (cell - low).abs() < 0.000_001,
                "band {band}: {cell} vs {low}"
            );
        }
    }
}
