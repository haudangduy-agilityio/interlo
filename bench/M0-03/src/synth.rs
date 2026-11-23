//! The synthetic test file: 3.5 s of silence and two tone bursts, no random numbers, so the same
//! command always gives the same bytes. No human voice (D-51).

use crate::wav;

/// 0.5 s silence, 1.0 s tone, 0.5 s silence, 1.0 s tone, 0.5 s silence at 16 kHz.
pub fn samples() -> Vec<i16> {
    let mut out = silence(8000);
    out.extend(tone(16_000));
    out.extend(silence(8000));
    out.extend(tone(16_000));
    out.extend(silence(8000));
    out
}

fn silence(count: usize) -> Vec<i16> {
    vec![0; count]
}

/// A 440 Hz sine at amplitude 0.3, starting at phase zero.
fn tone(count: usize) -> Vec<i16> {
    (0..count)
        .map(|n| {
            let phase = 2.0 * std::f64::consts::PI * 440.0 * n as f64 / f64::from(wav::RATE_HZ);
            (0.3 * phase.sin() * 32767.0).round() as i16
        })
        .collect()
}

/// The WAV file bytes of `samples()`.
pub fn wav_bytes() -> Vec<u8> {
    wav::encode(&samples())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_twice_gives_same_bytes() {
        assert_eq!(wav_bytes(), wav_bytes());
    }

    #[test]
    fn synth_header_is_44_bytes_and_pcm() {
        let bytes = wav_bytes();
        assert_eq!(bytes.len(), 112_044);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1, "format");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1, "channels");
        assert_eq!(
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
            16_000
        );
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 16, "bits");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(
            u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]),
            112_000
        );
    }

    #[test]
    fn synth_has_two_tone_bursts() {
        let s = samples();
        assert_eq!(s.len(), 56_000);
        assert!(s[..8000].iter().all(|v| *v == 0));
        assert!(s[8000..24000].iter().any(|v| v.abs() > 9000));
        assert!(s[24000..32000].iter().all(|v| *v == 0));
        assert!(s[32000..48000].iter().any(|v| v.abs() > 9000));
        assert!(s[48000..].iter().all(|v| *v == 0));
    }
}
