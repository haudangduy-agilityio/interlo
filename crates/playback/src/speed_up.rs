//! Speeds up the audio of one segment and keeps its pitch.
//!
//! Which side of the audio callback: CONTROL SIDE ONLY. `speed_up` allocates,
//! so a real-time audio callback must never call it (guardrail 8).

use crate::queue::MAX_SPEED_FACTOR;
use std::num::{NonZeroU16, NonZeroU32};

/// Input shorter than this is not sped up.
pub const MIN_SPEED_UP_MS: u32 = 100;

/// Silence put before and after the input. The time stretch fades its first
/// frame in and loses what lies past its last whole frame; both then happen
/// inside the silence.
const PAD_MS: u32 = 90;
/// The kept part begins and ends this long after the places that the first and
/// the last input sample have after a plain division by the speed; the start is
/// never later than one padding length (see `from`). The values come from
/// measured offsets of the time stretch; with them the kept part holds both ends
/// of the input. Measure them again when the pinned version of the dependency
/// changes.
const KEEP_FROM_MS: u32 = 6;
const KEEP_TO_MS: u32 = 15;
/// The engines give mono samples.
const MONO: NonZeroU16 = NonZeroU16::MIN;

/// Returns `samples` (one channel) played `speed` times faster at the same
/// pitch. A speed above `MAX_SPEED_FACTOR` counts as that factor.
///
/// The samples come back unchanged when the speed is not above 1.0 (also when
/// it is not a number), the sample rate is 0, the input is shorter than
/// `MIN_SPEED_UP_MS`, or the time stretch fails or returns less than the kept
/// part, so no segment is lost.
///
/// It works on the whole segment at once, and its time grows with the length and
/// with the square of the sample rate: compute the result before the segment is
/// due. At a speed of 1.0 or less it only copies the samples, so a caller has no
/// need to call it then.
pub fn speed_up(samples: &[f32], sample_rate: u32, speed: f32) -> Vec<f32> {
    try_speed_up(samples, sample_rate, speed).unwrap_or_else(|| samples.to_vec())
}

/// `None` when the samples must stay as they are.
fn try_speed_up(samples: &[f32], sample_rate: u32, speed: f32) -> Option<Vec<f32>> {
    let rate = NonZeroU32::new(sample_rate)?;
    let too_short = samples.len() < samples_in(MIN_SPEED_UP_MS, rate);
    if speed.is_nan() || speed <= 1.0 || too_short {
        return None;
    }
    let speed = speed.min(MAX_SPEED_FACTOR);

    let pad = samples_in(PAD_MS, rate);
    let padded_len = samples.len() + 2 * pad;
    let mut padded = Vec::with_capacity(padded_len);
    padded.resize(pad, 0.0);
    padded.extend_from_slice(samples);
    padded.resize(padded_len, 0.0);
    let output = wsola::stretch(&padded, rate, MONO, speed).ok()?;

    let position_after_stretch = |len: usize| (len as f64 / f64::from(speed)).round() as usize;
    // Never later than one padding length into the output: at a speed near 1.0
    // the plain place plus `KEEP_FROM_MS` comes after the first place where the
    // time stretch can put input.
    let from = (position_after_stretch(pad) + samples_in(KEEP_FROM_MS, rate)).min(pad);
    let to = position_after_stretch(pad + samples.len()) + samples_in(KEEP_TO_MS, rate);
    output.get(from..to).map(<[f32]>::to_vec)
}

/// How many samples `ms` milliseconds hold at `rate`, rounded down.
fn samples_in(ms: u32, rate: NonZeroU32) -> usize {
    let samples = u64::from(rate.get()) * u64::from(ms) / 1_000;
    usize::try_from(samples).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATES: [u32; 2] = [16_000, 24_000];
    const PEAK: f32 = 0.8;

    /// A sine tone of `len` samples.
    fn tone(sample_rate: u32, len: usize, hz: f32) -> Vec<f32> {
        (0..len)
            .map(|i| PEAK * (TAU * hz * i as f32 / sample_rate as f32).sin())
            .collect()
    }

    /// Frequency by rising zero crossings: the periods between the first and
    /// the last crossing over the time they take, so silence at either end of
    /// the samples does not count. 0.0 with fewer than two crossings.
    fn frequency_hz(samples: &[f32], sample_rate: u32) -> f32 {
        let rising: Vec<usize> = (1..samples.len())
            .filter(|&i| samples[i - 1] < 0.0 && samples[i] >= 0.0)
            .collect();
        match rising[..] {
            [first, .., last] => {
                (rising.len() - 1) as f32 * sample_rate as f32 / (last - first) as f32
            }
            _ => 0.0,
        }
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |max, s| max.max(s.abs()))
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum()
    }

    /// One second of silence at 16 000 Hz with a 20 ms tone burst at `from`.
    fn silence_with_burst(from: usize) -> Vec<f32> {
        let mut samples = vec![0.0; 16_000];
        samples[from..from + 320].copy_from_slice(&tone(16_000, 320, 1_000.0));
        samples
    }

    #[test]
    fn speed_up_keeps_the_pitch_of_a_tone() {
        for rate in RATES {
            let input = tone(rate, rate as usize, 200.0);
            let output = speed_up(&input, rate, MAX_SPEED_FACTOR);
            assert!(output.len() < input.len(), "{rate} Hz: not sped up");
            let hz = frequency_hz(&output, rate);
            assert!((hz - 200.0).abs() <= 4.0, "{rate} Hz: {hz} Hz");
        }
    }

    #[test]
    fn speed_up_keeps_the_end_of_a_sentence() {
        let input = silence_with_burst(16_000 - 320);
        let output = speed_up(&input, 16_000, MAX_SPEED_FACTOR);
        assert!(output.len() < input.len(), "not sped up");
        let last_40_ms = &output[output.len() - 640..];
        assert!(
            peak(last_40_ms) >= peak(&input) / 2.0,
            "peak {}",
            peak(last_40_ms)
        );
        // The burst is the only sound; ideal speed-up keeps 1 / speed of its
        // energy (0.91 at 1.10), while a cut into the burst loses more.
        let energy_ratio = energy(&output) / energy(&input);
        assert!(energy_ratio >= 0.8, "energy ratio {energy_ratio}");
    }

    #[test]
    fn speed_up_keeps_the_start_of_a_sentence() {
        let input = silence_with_burst(0);
        let output = speed_up(&input, 16_000, MAX_SPEED_FACTOR);
        assert!(output.len() < input.len(), "not sped up");
        let first_40_ms = &output[..640];
        assert!(
            peak(first_40_ms) >= peak(&input) / 2.0,
            "peak {}",
            peak(first_40_ms)
        );
    }

    #[test]
    fn a_speed_near_one_keeps_from_one_padding_length_on() {
        let input = tone(16_000, 16_000, 200.0);
        // At 1.02 the padding is 1 440 samples and the kept part ends at
        // 17 098 + 240; it must not start after sample 1 440.
        let output = speed_up(&input, 16_000, 1.02);
        assert_eq!(output.len(), 17_338 - 1_440);
    }

    #[test]
    fn at_the_cap_the_kept_part_starts_before_one_padding_length() {
        let input = tone(16_000, 16_000, 200.0);
        // At 1.10 the plain start is round(1 440 / 1.10) + 96 = 1 405,
        // before the 1 440-sample limit. The kept part ends at
        // round(17 440 / 1.10) + 240 = 16 095.
        let output = speed_up(&input, 16_000, MAX_SPEED_FACTOR);
        assert_eq!(output.len(), 16_095 - 1_405);
    }

    #[test]
    fn speed_of_one_or_less_returns_the_samples_unchanged() {
        let input = tone(16_000, 16_000, 200.0);
        for speed in [1.0, 0.5, f32::NAN] {
            assert!(speed_up(&input, 16_000, speed) == input, "speed {speed}");
        }
    }

    #[test]
    fn speed_above_the_cap_counts_as_the_cap() {
        let input = tone(16_000, 16_000, 200.0);
        let at_cap = speed_up(&input, 16_000, MAX_SPEED_FACTOR);
        assert!(at_cap.len() < input.len(), "not sped up");
        assert!(speed_up(&input, 16_000, 2.0) == at_cap);
    }

    #[test]
    fn short_input_and_zero_rate_come_back_unchanged() {
        let one_second = tone(16_000, 16_000, 200.0);
        // 100 ms are 1 600 samples at 16 000 Hz.
        let unchanged: [(&str, &[f32], u32); 3] = [
            ("empty", &[], 16_000),
            ("one sample short of 100 ms", &one_second[..1_599], 16_000),
            ("sample rate 0", &one_second, 0),
        ];
        for (case, input, rate) in unchanged {
            assert!(speed_up(input, rate, MAX_SPEED_FACTOR) == input, "{case}");
        }

        let output = speed_up(&one_second[..1_600], 16_000, MAX_SPEED_FACTOR);
        assert!(output.len() < 1_600, "{} samples", output.len());
    }

    #[test]
    fn speed_up_shortens_a_sentence_by_the_speed() {
        for rate in RATES {
            let input = tone(rate, rate as usize, 200.0);
            let output = speed_up(&input, rate, MAX_SPEED_FACTOR);
            let expected = input.len() as f32 / MAX_SPEED_FACTOR;
            assert!(
                (output.len() as f32 - expected).abs() <= expected * 0.02,
                "{rate} Hz: {} samples, expected about {expected}",
                output.len()
            );
        }
    }
}
