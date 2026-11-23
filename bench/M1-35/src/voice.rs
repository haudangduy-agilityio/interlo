//! Role `voice`: speaks the test sentence and writes 16 kHz mono f32 little-endian samples to
//! stdout. Pass rule (AC-10): at least 4800 samples (0.3 s) with a peak amplitude above 0.01.
//!
//! The model is a set of ONNX files and a two-level loop: a text encoder runs once; then for each
//! audio frame a global step (`prefix_step`) and a local step (`local_frame_decode`, sampling is
//! inside the graph) run; the collected frame codes go through the decoder of the audio codec.
//! The loop follows the model host's reference code (MIT). Preset voices are precomputed, so the
//! voice encoder file is not needed.

use std::io::Write;
use std::path::Path;
use std::time::Instant;

use ort::environment::Environment;
use ort::session::{Session, SessionOutputs};
use ort::value::{DynValue, PrimitiveTensorElementType, Tensor};
use tokenizers::Tokenizer;

use crate::common::*;

// The exported graphs hardcode these ids (tokenizer contract of the model).
const BOS: i64 = 1;
const EOT: i64 = 2;
const MAX_TEXT_IDS: usize = 512;
const CODEC_RATE_HZ: usize = 48_000;
const LOW_PASS_HZ: f64 = 7_500.0;

const MIN_FRAMES: usize = 4;
const MAX_FRAMES: usize = 300;
const TEXT_TEMPERATURE: f32 = 1.0;
const TEXT_TOPK: i64 = 50;
const AUDIO_TEMPERATURE: f32 = 0.8;
const AUDIO_TOPK: i64 = 25;
const AUDIO_TOPP: f32 = 0.95;
const REPETITION_PENALTY: f32 = 1.2;
const CFG_SCALE: f32 = 1.0;

const MIN_SAMPLES: usize = 4_800;
const MIN_PEAK: f32 = 0.01;

struct Params {
    codebooks: usize,
    codebook_size: usize,
    d_model: usize,
    heads: usize,
    layers: usize,
    voice_queries: usize,
}

struct Floats {
    shape: Vec<usize>,
    data: Vec<f32>,
}

struct Flags {
    shape: Vec<usize>,
    data: Vec<bool>,
}

/// Small deterministic generator for the random draws the sampling graph asks for.
struct Rng(u64);

impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let x = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        ((x >> 40) as f32) / (1u64 << 24) as f32
    }
}

fn tensor<T: PrimitiveTensorElementType + std::fmt::Debug + Clone + 'static>(
    shape: &[usize],
    data: Vec<T>,
) -> Res<DynValue> {
    Ok(Tensor::from_array((shape.to_vec(), data))
        .ctx("tensor")?
        .into_dyn())
}

fn floats_out(outputs: &SessionOutputs, name: &str) -> Res<Floats> {
    let (shape, data) = outputs
        .get(name)
        .ok_or_else(|| format!("output missing: {name}"))?
        .try_extract_tensor::<f32>()
        .ctx(name)?;
    Ok(Floats {
        shape: dims(shape),
        data: data.to_vec(),
    })
}

fn flags_out(outputs: &SessionOutputs, name: &str) -> Res<Flags> {
    let (shape, data) = outputs
        .get(name)
        .ok_or_else(|| format!("output missing: {name}"))?
        .try_extract_tensor::<bool>()
        .ctx(name)?;
    Ok(Flags {
        shape: dims(shape),
        data: data.to_vec(),
    })
}

pub fn run(cfg: &Config) -> Res<Outcome> {
    let t_load = Instant::now();
    let dir = cfg.models_dir.join("voice");
    let env = new_environment()?;
    let mut text_encoder = load_session(&env, &dir.join("text_encoder.onnx"))?;
    let mut prefix_step = load_session(&env, &dir.join("prefix_step.onnx"))?;
    let mut local_decode = load_session(&env, &dir.join("local_frame_decode.onnx"))?;
    let mut codec = load_codec(&env, &dir.join("codec"))?;
    let config = read_text(&dir.join("config.json"))?;
    let params = Params {
        codebooks: json_usize(&config, "num_codebooks")?,
        codebook_size: json_usize(&config, "codebook_size")?,
        d_model: json_usize(&config, "d_model")?,
        heads: json_usize(&config, "n_heads")?,
        layers: json_usize(&config, "n_layers")?,
        voice_queries: json_usize(&config, "n_voice_queries")?,
    };
    let sample_rate = json_usize(&config, "sample_rate")?;
    if sample_rate != CODEC_RATE_HZ {
        return Err(format!("unexpected codec rate {sample_rate}"));
    }
    let tokenizer =
        Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| format!("tokenizer: {e}"))?;
    let voice = read_f32_raw(&dir.join("voices/preset.bin"))?;
    if voice.len() != params.voice_queries * params.d_model {
        return Err(format!("voice preset has {} values", voice.len()));
    }
    let load_ms = ms(t_load);

    let text = sentence(&cfg.reading_list)?;

    let t_first = Instant::now();
    let ids = text_ids(&tokenizer, &text)?;
    let codes = generate_codes(
        &mut text_encoder,
        &mut prefix_step,
        &mut local_decode,
        &params,
        &ids,
        &voice,
    )?;
    let wide = decode_codec(&mut codec, &codes, params.codebooks)?;
    let samples = downsample_3(&wide);
    let first_ms = ms(t_first);

    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let mut out = Outcome::new(load_ms, first_ms);
    out.detail = format!("samples={} peak={peak:.3}", samples.len());
    out.pass = samples.len() >= MIN_SAMPLES && peak > MIN_PEAK;
    if !out.pass {
        out.reason = format!(
            "need at least {MIN_SAMPLES} samples and a peak above {MIN_PEAK}: {}",
            out.detail
        );
    }

    refuse_file_stdout()?;
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in &samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&bytes).ctx("write samples")?;
    stdout.flush().ctx("flush samples")?;
    Ok(out)
}

/// The codec graph stores the name of its external weight file; the file of the download list has
/// a shorter name. Read the stored name from the graph and link it to the short file once (same
/// bytes, no copy), inside the models folder. A file that already has the long name must be that
/// same file, so the checksum of the list covers what is loaded.
fn load_codec(env: &Environment, dir: &Path) -> Res<Session> {
    use std::os::unix::fs::MetadataExt;
    let graph = dir.join("decode_full.onnx");
    let short = dir.join("decode_shared.data");
    if short.is_file() && graph.is_file() {
        let bytes = std::fs::read(&graph).ctx("read codec graph")?;
        let name = external_data_name(&bytes).ok_or("codec graph names no weight file")?;
        let long = dir.join(name);
        if long.exists() {
            let (a, b) = (
                std::fs::metadata(&short).ctx("codec weights")?,
                std::fs::metadata(&long).ctx("codec weights link")?,
            );
            if (a.dev(), a.ino()) != (b.dev(), b.ino()) {
                return Err("codec weights link is not the listed file".to_string());
            }
        } else {
            std::fs::hard_link(&short, &long).ctx("link codec weights")?;
        }
    }
    load_session(env, &graph)
}

/// Finds the weight file name stored in the graph: a run of name characters that ends with
/// `_decode_shared.data`. Only plain file names are accepted, never a path.
fn external_data_name(graph: &[u8]) -> Option<String> {
    const SUFFIX: &[u8] = b"_decode_shared.data";
    let end = graph.windows(SUFFIX.len()).position(|w| w == SUFFIX)? + SUFFIX.len();
    let start = graph[..end]
        .iter()
        .rposition(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
        .map_or(0, |i| i + 1);
    let name = std::str::from_utf8(&graph[start..end]).ok()?;
    Some(name.to_string())
}

/// Audio is pipe-only (guardrail 2): refuse to write samples into a regular file.
fn refuse_file_stdout() -> Res<()> {
    use std::os::fd::AsFd;
    let owned = std::io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .ctx("stdout")?;
    let is_file = std::fs::File::from(owned)
        .metadata()
        .ctx("stdout")?
        .is_file();
    if is_file {
        return Err("stdout is a regular file; samples go to a pipe only".to_string());
    }
    Ok(())
}

/// Reads raw f32 little-endian values.
fn read_f32_raw(path: &Path) -> Res<Vec<f32>> {
    let bytes = std::fs::read(path).ctx(&format!("read {}", path.display()))?;
    Ok(f32s_from_le(&bytes))
}

/// Collapses white space, composes the text, and wraps the token ids as `[BOS | body | EOT]`.
fn text_ids(tokenizer: &Tokenizer, text: &str) -> Res<Vec<i64>> {
    let composed = nfc(text)?;
    let collapsed = composed.split_whitespace().collect::<Vec<_>>().join(" ");
    let encoding = tokenizer
        .encode(collapsed, false)
        .map_err(|e| format!("tokenize: {e}"))?;
    let mut ids = vec![BOS];
    ids.extend(
        encoding
            .get_ids()
            .iter()
            .take(MAX_TEXT_IDS)
            .map(|&i| i as i64),
    );
    ids.push(EOT);
    Ok(ids)
}

struct TextState {
    cross_kv: Floats,
    text_valid: Flags,
}

fn generate_codes(
    text_encoder: &mut Session,
    prefix_step: &mut Session,
    local_decode: &mut Session,
    p: &Params,
    ids: &[i64],
    voice: &[f32],
) -> Res<Vec<Vec<i32>>> {
    let len = ids.len();
    let enc = text_encoder
        .run(vec![
            ("text_ids", tensor(&[1, len], ids.to_vec())?),
            ("txt_lengths", tensor(&[1], vec![len as i64])?),
        ])
        .ctx("text encoder run")?;
    let soa = floats_out(&enc, "soa_embed")?;
    let state = TextState {
        cross_kv: floats_out(&enc, "cross_kv")?,
        text_valid: flags_out(&enc, "text_valid")?,
    };
    drop(enc);

    // Prefix: [voice | start-of-audio] from an empty cache.
    let v = p.voice_queries;
    let t = v + 1;
    let mut external = voice.to_vec();
    external.extend_from_slice(&soa.data);
    if external.len() != t * p.d_model {
        return Err(format!("prefix has {} values", external.len()));
    }
    let mut bidirectional = vec![true; v];
    bidirectional.push(false);
    let (mut hidden, mut packed_kv, mut full_valid) = prefix_call(
        prefix_step,
        p,
        &state,
        Step {
            embed: external,
            use_embed: vec![true; t],
            codes: vec![0i64; t * p.codebooks],
            positions: (0..t as i64).collect(),
            bidirectional,
            len: t,
        },
        Floats {
            shape: vec![p.layers, 2, 1, p.heads, 0, p.d_model / p.heads],
            data: Vec::new(),
        },
        Flags {
            shape: vec![1, 0],
            data: Vec::new(),
        },
    )?;

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut seen = vec![false; p.codebooks * p.codebook_size];
    let mut frames: Vec<Vec<i32>> = Vec::new();
    let mut index = 0usize;
    loop {
        let forbid = index < MIN_FRAMES;
        let (is_eoa, codes) = local_call(local_decode, p, &hidden, forbid, &mut seen, &mut rng)?;
        if index >= MAX_FRAMES {
            break;
        }
        // The frame that carries the end marker is kept: it is the model's own trailing frame.
        let step_codes: Vec<i64> = codes.iter().map(|&c| c as i64).collect();
        frames.push(codes);
        if is_eoa {
            break;
        }
        let next = prefix_call(
            prefix_step,
            p,
            &state,
            Step {
                embed: vec![0.0f32; p.d_model],
                use_embed: vec![false],
                codes: step_codes,
                positions: vec![(v + 1 + index) as i64],
                bidirectional: vec![false],
                len: 1,
            },
            packed_kv,
            full_valid,
        )?;
        hidden = next.0;
        packed_kv = next.1;
        full_valid = next.2;
        index += 1;
    }
    if frames.is_empty() {
        return Err("no audio frames".to_string());
    }
    Ok(frames)
}

struct Step {
    embed: Vec<f32>,
    use_embed: Vec<bool>,
    codes: Vec<i64>,
    positions: Vec<i64>,
    bidirectional: Vec<bool>,
    len: usize,
}

/// One call of the global step. Returns the last hidden row, the grown cache and its validity.
fn prefix_call(
    session: &mut Session,
    p: &Params,
    text: &TextState,
    step: Step,
    packed_kv: Floats,
    past_valid: Flags,
) -> Res<(Vec<f32>, Floats, Flags)> {
    let n = step.len;
    let inputs = vec![
        ("external_embed", tensor(&[1, n, p.d_model], step.embed)?),
        ("use_external_embed", tensor(&[1, n], step.use_embed)?),
        ("frame_codes", tensor(&[1, n, p.codebooks], step.codes)?),
        ("new_pos", tensor(&[1, n], step.positions)?),
        ("new_valid", tensor(&[1, n], vec![true; n])?),
        ("new_bidirectional", tensor(&[1, n], step.bidirectional)?),
        ("packed_kv", tensor(&packed_kv.shape, packed_kv.data)?),
        ("past_valid", tensor(&past_valid.shape, past_valid.data)?),
        (
            "cross_kv",
            tensor(&text.cross_kv.shape, text.cross_kv.data.clone())?,
        ),
        (
            "text_valid",
            tensor(&text.text_valid.shape, text.text_valid.data.clone())?,
        ),
    ];
    let out = session.run(inputs).ctx("prefix step run")?;
    let hidden = floats_out(&out, "hidden")?;
    if hidden.data.len() < p.d_model {
        return Err("hidden state is too short".to_string());
    }
    let last = hidden.data[hidden.data.len() - p.d_model..].to_vec();
    Ok((
        last,
        floats_out(&out, "new_packed_kv")?,
        flags_out(&out, "full_valid")?,
    ))
}

/// One call of the local step: the control channel and all codebooks of one frame.
fn local_call(
    session: &mut Session,
    p: &Params,
    hidden: &[f32],
    forbid_eoa: bool,
    seen: &mut [bool],
    rng: &mut Rng,
) -> Res<(bool, Vec<i32>)> {
    let draws: Vec<f32> = (0..p.codebooks).map(|_| rng.next_f32()).collect();
    let inputs = vec![
        ("global_hidden", tensor(&[1, p.d_model], hidden.to_vec())?),
        ("forbid_eoa", tensor(&[1], vec![forbid_eoa])?),
        ("text_temperature", tensor(&[1], vec![TEXT_TEMPERATURE])?),
        ("text_topk", tensor(&[1], vec![TEXT_TOPK])?),
        ("audio_temperature", tensor(&[1], vec![AUDIO_TEMPERATURE])?),
        ("audio_topk", tensor(&[1], vec![AUDIO_TOPK])?),
        ("audio_topp", tensor(&[1], vec![AUDIO_TOPP])?),
        (
            "audio_repetition_penalty",
            tensor(&[1], vec![REPETITION_PENALTY])?,
        ),
        (
            "seen_mask",
            tensor(&[1, p.codebooks, p.codebook_size], seen.to_vec())?,
        ),
        ("ctrl_random_u", tensor(&[1], vec![rng.next_f32()])?),
        ("audio_random_u", tensor(&[1, p.codebooks], draws)?),
        ("cfg_scale", tensor(&[1], vec![CFG_SCALE])?),
    ];
    let out = session.run(inputs).ctx("local step run")?;
    let eoa = out
        .get("is_eoa")
        .ok_or("output missing: is_eoa")?
        .try_extract_tensor::<i32>()
        .ctx("is_eoa")?
        .1
        .first()
        .copied()
        .ok_or("is_eoa is empty")?;
    let codes: Vec<i32> = out
        .get("codes")
        .ok_or("output missing: codes")?
        .try_extract_tensor::<i32>()
        .ctx("codes")?
        .1
        .to_vec();
    if codes.len() != p.codebooks {
        return Err(format!("frame has {} codes", codes.len()));
    }
    for (book, &code) in codes.iter().enumerate() {
        if code < 0 || code as usize >= p.codebook_size {
            return Err(format!("code {code} is outside the codebook"));
        }
        let at = book * p.codebook_size + code as usize;
        if let Some(slot) = seen.get_mut(at) {
            *slot = true;
        }
    }
    Ok((eoa != 0, codes))
}

/// Codec decoder: frame codes (time, codebook) to wide samples at 48 kHz, channels averaged.
fn decode_codec(codec: &mut Session, frames: &[Vec<i32>], books: usize) -> Res<Vec<f32>> {
    let t = frames.len();
    let flat: Vec<i32> = frames.iter().flatten().copied().collect();
    let out = codec
        .run(vec![
            ("audio_codes", tensor(&[1, t, books], flat)?),
            ("audio_code_lengths", tensor(&[1], vec![t as i32])?),
        ])
        .ctx("codec run")?;
    let audio = floats_out(&out, "audio")?;
    let used = out
        .get("audio_lengths")
        .ok_or("output missing: audio_lengths")?
        .try_extract_tensor::<i32>()
        .ctx("audio_lengths")?
        .1
        .first()
        .copied()
        .ok_or("audio_lengths is empty")?
        .max(0) as usize;
    if audio.shape.len() != 3 {
        return Err("audio has an unexpected shape".to_string());
    }
    let (channels, total) = (audio.shape[1], audio.shape[2]);
    let used = used.min(total);
    let mut mono = vec![0.0f32; used];
    for c in 0..channels {
        let row = &audio.data[c * total..c * total + used];
        for (m, v) in mono.iter_mut().zip(row) {
            *m += v / channels as f32;
        }
    }
    Ok(mono)
}

/// 48 kHz to 16 kHz: a windowed-sinc low-pass filter, then every third sample.
fn downsample_3(x: &[f32]) -> Vec<f32> {
    const TAPS: usize = 96;
    let cutoff = LOW_PASS_HZ / CODEC_RATE_HZ as f64;
    let mid = (TAPS as f64 - 1.0) / 2.0;
    let mut h: Vec<f64> = (0..TAPS)
        .map(|n| {
            let d = n as f64 - mid;
            let sinc = if d == 0.0 {
                2.0 * cutoff
            } else {
                (2.0 * std::f64::consts::PI * cutoff * d).sin() / (std::f64::consts::PI * d)
            };
            let window =
                0.54 - 0.46 * (2.0 * std::f64::consts::PI * n as f64 / (TAPS as f64 - 1.0)).cos();
            sinc * window
        })
        .collect();
    let sum: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= sum);
    let half = TAPS / 2;
    (0..x.len() / 3)
        .map(|k| {
            let centre = (3 * k) as isize;
            let mut acc = 0.0f64;
            for (j, coef) in h.iter().enumerate() {
                let at = centre + j as isize - half as isize;
                if at >= 0 && (at as usize) < x.len() {
                    acc += coef * x[at as usize] as f64;
                }
            }
            acc as f32
        })
        .collect()
}
