//! Role `asr`: recognizes 16 kHz samples read from stdin and compares the words with the test
//! sentence. Pass rule (AC-11): the text is not empty and at least half of the words of the input
//! sentence (compared as sets of lower-cased, punctuation-free, composed strings) appear in it.
//! This is a load-and-run check, not a word error measurement (M0-05).
//!
//! Features are a log-mel spectrogram of 80 bands over a 30 second window (400-point windows, hop
//! 160), computed here with a plain transform. The decoder file is a merged decoder; like the
//! translation role, every step runs the whole prefix with the cache branch off.

use std::collections::HashSet;
use std::f64::consts::PI;
use std::time::Instant;

use ort::session::Session;
use ort::value::{DynValue, Tensor};
use tokenizers::Tokenizer;

use crate::common::*;

const SAMPLE_RATE_HZ: usize = 16_000;
const N_FFT: usize = 400;
const HOP: usize = 160;
const N_MELS: usize = 80;
const N_BINS: usize = N_FFT / 2 + 1;
const WINDOW_SAMPLES: usize = 30 * SAMPLE_RATE_HZ;
const FRAMES: usize = WINDOW_SAMPLES / HOP;
const MAX_NEW_TOKENS: usize = 120;
const MIN_SHARE: f64 = 0.5;

pub fn run(cfg: &Config) -> Res<Outcome> {
    let t_load = Instant::now();
    let dir = cfg.models_dir.join("asr");
    let env = new_environment()?;
    let mut encoder = load_session(&env, &dir.join("encoder.onnx"))?;
    let mut decoder = load_session(&env, &dir.join("decoder.onnx"))?;
    let tokenizer =
        Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| format!("tokenizer: {e}"))?;
    let config = read_text(&dir.join("config.json"))?;
    let start = json_int(&config, "decoder_start_token_id")?;
    let eos = json_int(&config, "eos_token_id")?;
    let token = |name: &str| -> Res<i64> {
        tokenizer
            .token_to_id(name)
            .map(i64::from)
            .ok_or_else(|| format!("token missing: {name}"))
    };
    // Prompt: start, language, task, no time stamps.
    let prompt = vec![
        start,
        token("<|vi|>")?,
        token("<|transcribe|>")?,
        token("<|notimestamps|>")?,
    ];
    let load_ms = ms(t_load);

    let samples = read_samples_from_stdin()?;
    let expected = sentence(&cfg.reading_list)?;

    let t_first = Instant::now();
    let features = log_mel(&samples);
    let text = recognize(
        &mut encoder,
        &mut decoder,
        &tokenizer,
        features,
        prompt,
        eos,
    )?;
    let first_ms = ms(t_first);

    let share = word_share(&expected, &text)?;
    let mut out = Outcome::new(load_ms, first_ms);
    out.detail = format!(
        "share={share:.3} words_out={}",
        text.split_whitespace().count()
    );
    out.pass = !text.trim().is_empty() && share >= MIN_SHARE;
    if !out.pass {
        out.reason = if text.trim().is_empty() {
            "output is empty".to_string()
        } else {
            format!("share {share:.3} is below {MIN_SHARE}")
        };
    }
    if cfg.show_text {
        eprintln!("TEXT={}", text.trim());
    }
    Ok(out)
}

fn recognize(
    encoder: &mut Session,
    decoder: &mut Session,
    tokenizer: &Tokenizer,
    features: Vec<f32>,
    prompt: Vec<i64>,
    eos: i64,
) -> Res<String> {
    let enc_out = encoder
        .run(vec![(
            "input_features",
            Tensor::from_array(([1usize, N_MELS, FRAMES], features))
                .ctx("features")?
                .into_dyn(),
        )])
        .ctx("encoder run")?;
    let (enc_shape, enc_data) = enc_out
        .get("last_hidden_state")
        .ok_or("encoder output missing")?
        .try_extract_tensor::<f32>()
        .ctx("encoder output")?;
    let enc_dims = dims(enc_shape);
    if enc_dims.len() != 3 {
        return Err("encoder output has an unexpected shape".to_string());
    }
    let (steps, width) = (enc_dims[1], enc_dims[2]);
    let hidden: Vec<f32> = enc_data.to_vec();
    drop(enc_out);

    let mut decoded = prompt;
    let mut generated: Vec<u32> = Vec::new();
    for _ in 0..MAX_NEW_TOKENS {
        let t = decoded.len();
        let mut inputs: Vec<(String, DynValue)> = vec![
            (
                "input_ids".to_string(),
                Tensor::from_array(([1usize, t], decoded.clone()))
                    .ctx("decoder ids")?
                    .into_dyn(),
            ),
            (
                "encoder_hidden_states".to_string(),
                Tensor::from_array(([1usize, steps, width], hidden.clone()))
                    .ctx("hidden")?
                    .into_dyn(),
            ),
            (
                "use_cache_branch".to_string(),
                Tensor::from_array(([1usize], vec![false]))
                    .ctx("branch flag")?
                    .into_dyn(),
            ),
        ];
        push_empty_past(&mut inputs)?;
        let outputs = decoder.run(inputs).ctx("decoder run")?;
        let (shape, logits) = outputs
            .get("logits")
            .ok_or("logits missing")?
            .try_extract_tensor::<f32>()
            .ctx("logits")?;
        let vocab = *dims(shape).last().ok_or("logits shape is empty")?;
        if vocab == 0 || logits.len() < vocab {
            return Err("logits are empty".to_string());
        }
        let mut last = logits[logits.len() - vocab..].to_vec();
        // Only text tokens and the end token may follow the prompt: every id above the end token
        // is a control or time stamp token.
        for (id, v) in last.iter_mut().enumerate() {
            if id as i64 > eos {
                *v = f32::NEG_INFINITY;
            }
        }
        let next = argmax(&last) as i64;
        if next == eos {
            break;
        }
        decoded.push(next);
        generated.push(next as u32);
    }
    tokenizer
        .decode(&generated, true)
        .map_err(|e| format!("detokenize: {e}"))
}

/// Log-mel features of the samples, padded with zeros or cut to 30 seconds. Layout: band-major,
/// `N_MELS` rows of `FRAMES` values.
fn log_mel(samples: &[f32]) -> Vec<f32> {
    let mut padded = vec![0.0f32; WINDOW_SAMPLES];
    let n = samples.len().min(WINDOW_SAMPLES);
    padded[..n].copy_from_slice(&samples[..n]);

    // Reflect padding of N_FFT/2 on both sides (centred frames).
    let pad = N_FFT / 2;
    let mut x = Vec::with_capacity(WINDOW_SAMPLES + 2 * pad);
    for i in (1..=pad).rev() {
        x.push(padded[i]);
    }
    x.extend_from_slice(&padded);
    for i in 0..pad {
        x.push(padded[WINDOW_SAMPLES - 2 - i]);
    }

    // Periodic Hann window and the transform tables.
    let window: Vec<f64> = (0..N_FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / N_FFT as f64).cos())
        .collect();
    let mut cos_table = vec![0.0f64; N_BINS * N_FFT];
    let mut sin_table = vec![0.0f64; N_BINS * N_FFT];
    for k in 0..N_BINS {
        for i in 0..N_FFT {
            let angle = 2.0 * PI * (k * i) as f64 / N_FFT as f64;
            cos_table[k * N_FFT + i] = angle.cos();
            sin_table[k * N_FFT + i] = angle.sin();
        }
    }
    let filters = mel_filters();

    let mut log_spec = vec![0.0f64; N_MELS * FRAMES];
    let mut frame = vec![0.0f64; N_FFT];
    let mut power = vec![0.0f64; N_BINS];
    for f in 0..FRAMES {
        for i in 0..N_FFT {
            frame[i] = x[f * HOP + i] as f64 * window[i];
        }
        for k in 0..N_BINS {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            let cos_row = &cos_table[k * N_FFT..(k + 1) * N_FFT];
            let sin_row = &sin_table[k * N_FFT..(k + 1) * N_FFT];
            for i in 0..N_FFT {
                re += frame[i] * cos_row[i];
                im += frame[i] * sin_row[i];
            }
            power[k] = re * re + im * im;
        }
        for m in 0..N_MELS {
            let row = &filters[m * N_BINS..(m + 1) * N_BINS];
            let energy: f64 = row.iter().zip(&power).map(|(a, b)| a * b).sum();
            log_spec[m * FRAMES + f] = energy.max(1e-10).log10();
        }
    }
    let top = log_spec.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    log_spec
        .iter()
        .map(|&v| ((v.max(top - 8.0) + 4.0) / 4.0) as f32)
        .collect()
}

/// Slaney mel scale: linear below 1 kHz, logarithmic above.
const MEL_F_SP: f64 = 200.0 / 3.0;
const MEL_LOG_HZ: f64 = 1000.0;

fn min_log_mel() -> f64 {
    MEL_LOG_HZ / MEL_F_SP
}

fn log_step() -> f64 {
    6.4f64.ln() / 27.0
}

fn hz_to_mel(hz: f64) -> f64 {
    let (f_sp, min_log_hz, min_log_mel, log_step) =
        (MEL_F_SP, MEL_LOG_HZ, min_log_mel(), log_step());
    if hz >= min_log_hz {
        min_log_mel + (hz / min_log_hz).ln() / log_step
    } else {
        hz / f_sp
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    let (f_sp, min_log_hz, min_log_mel, log_step) =
        (MEL_F_SP, MEL_LOG_HZ, min_log_mel(), log_step());
    if mel >= min_log_mel {
        min_log_hz * (log_step * (mel - min_log_mel)).exp()
    } else {
        f_sp * mel
    }
}

/// Triangular mel filters on a Slaney scale with Slaney area normalization, 0 to 8 kHz.
fn mel_filters() -> Vec<f64> {
    let (mel_min, mel_max) = (hz_to_mel(0.0), hz_to_mel(SAMPLE_RATE_HZ as f64 / 2.0));
    let points: Vec<f64> = (0..N_MELS + 2)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f64 / (N_MELS + 1) as f64))
        .collect();
    let bin_hz: Vec<f64> = (0..N_BINS)
        .map(|k| (SAMPLE_RATE_HZ as f64 / 2.0) * k as f64 / (N_BINS - 1) as f64)
        .collect();
    let mut filters = vec![0.0f64; N_MELS * N_BINS];
    for m in 0..N_MELS {
        let (lo, mid, hi) = (points[m], points[m + 1], points[m + 2]);
        let norm = 2.0 / (hi - lo);
        for (k, &hz) in bin_hz.iter().enumerate() {
            let down = (hz - lo) / (mid - lo);
            let up = (hi - hz) / (hi - mid);
            filters[m * N_BINS + k] = down.min(up).max(0.0) * norm;
        }
    }
    filters
}

/// Lower-cases, composes, and strips punctuation; returns the set of words.
fn words(text: &str) -> Res<HashSet<String>> {
    let normalized = nfc(&text.to_lowercase())?;
    let cleaned: String = normalized
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    Ok(cleaned.split_whitespace().map(str::to_string).collect())
}

/// Share of the words of `expected` that appear in `got`.
fn word_share(expected: &str, got: &str) -> Res<f64> {
    let want = words(expected)?;
    if want.is_empty() {
        return Err("the input sentence has no words".to_string());
    }
    let have = words(got)?;
    let hit = want.iter().filter(|w| have.contains(*w)).count();
    Ok(hit as f64 / want.len() as f64)
}
