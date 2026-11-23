//! WAV reader and writer. The input is PCM, 16-bit, mono, 16000 Hz and nothing else: the
//! harness never resamples or converts.

pub const RATE_HZ: u32 = 16_000;
const HEADER_BYTES: usize = 44;

/// Why a file was refused. `BadWav` is not a usable WAV file; `BadFormat` is a WAV file in a
/// format the harness does not read.
#[derive(Debug, PartialEq, Eq)]
pub enum WavError {
    BadWav,
    BadFormat,
}

/// Decode a WAV file into samples in the range -1.0 to 1.0.
pub fn parse(bytes: &[u8]) -> Result<Vec<f32>, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::BadWav);
    }
    let mut format: Option<(u16, u16, u32, u16)> = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = le_u32(&bytes[pos + 4..pos + 8]) as usize;
        let body = pos + 8;
        if body + size > bytes.len() {
            return Err(WavError::BadWav);
        }
        if id == b"fmt " {
            if size < 16 {
                return Err(WavError::BadWav);
            }
            let f = &bytes[body..body + 16];
            format = Some((
                le_u16(&f[0..2]),
                le_u16(&f[2..4]),
                le_u32(&f[4..8]),
                le_u16(&f[14..16]),
            ));
        } else if id == b"data" {
            let (tag, channels, rate, bits) = format.ok_or(WavError::BadWav)?;
            if tag != 1 || channels != 1 || rate != RATE_HZ || bits != 16 {
                return Err(WavError::BadFormat);
            }
            if !size.is_multiple_of(2) {
                return Err(WavError::BadWav);
            }
            return Ok(bytes[body..body + size]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                .collect());
        }
        pos = body + size + (size & 1);
    }
    Err(WavError::BadWav)
}

fn le_u16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Encode 16-bit mono 16 kHz samples as a WAV file (44 header bytes, then the samples).
pub fn encode(samples: &[i16]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(HEADER_BYTES + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE_HZ.to_le_bytes());
    out.extend_from_slice(&(RATE_HZ * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
pub fn make_wav(
    format: u16,
    channels: u16,
    rate: u32,
    bits: u16,
    data: &[u8],
    declared: u32,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + declared).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    let block = channels * bits / 8;
    out.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&declared.to_le_bytes());
    out.extend_from_slice(data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn wav_reads_pcm16_mono_16k() {
        let d = data(&[0, 16384, -16384, 32767]);
        let wav = make_wav(1, 1, 16000, 16, &d, 8);
        let samples = parse(&wav).unwrap();
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0], 0.0);
        assert_eq!(samples[1], 0.5);
        assert_eq!(samples[2], -0.5);
        assert!((samples[3] - 0.99997).abs() < 1e-4);
    }

    #[test]
    fn wav_rejects_two_channels() {
        let wav = make_wav(1, 2, 16000, 16, &data(&[0, 0]), 4);
        assert_eq!(parse(&wav), Err(WavError::BadFormat));
    }

    #[test]
    fn wav_rejects_8000_hz() {
        let wav = make_wav(1, 1, 8000, 16, &data(&[0, 0]), 4);
        assert_eq!(parse(&wav), Err(WavError::BadFormat));
    }

    #[test]
    fn wav_rejects_8_bit() {
        let wav = make_wav(1, 1, 16000, 8, &[128, 128], 2);
        assert_eq!(parse(&wav), Err(WavError::BadFormat));
    }

    #[test]
    fn wav_rejects_text_file() {
        assert_eq!(parse(b"not audio"), Err(WavError::BadWav));
        assert_eq!(parse(b""), Err(WavError::BadWav));
    }

    #[test]
    fn wav_rejects_truncated_data() {
        let wav = make_wav(1, 1, 16000, 16, &data(&[1, 2]), 100);
        assert_eq!(parse(&wav), Err(WavError::BadWav));
    }

    #[test]
    fn wav_rejects_file_without_data_chunk() {
        let wav = make_wav(1, 1, 16000, 16, &[], 0);
        assert_eq!(parse(&wav[..36]), Err(WavError::BadWav));
    }

    #[test]
    fn encode_then_parse_gives_the_same_samples() {
        let input = [0i16, 1, -1, 32767, -32768];
        let bytes = encode(&input);
        assert_eq!(bytes.len(), 44 + 10);
        let back = parse(&bytes).unwrap();
        let expected: Vec<f32> = input.iter().map(|s| f32::from(*s) / 32768.0).collect();
        assert_eq!(back, expected);
    }
}
