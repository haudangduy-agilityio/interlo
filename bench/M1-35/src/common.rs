//! Shared parts of the four roles: result line, model loading, the test sentence, the sample pipe.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ort::environment::Environment;
use ort::session::Session;
use ort::value::{DynValue, Tensor};
use tokenizers::{NormalizedString, Normalizer};

pub type Res<T> = Result<T, String>;

/// Adds a short context to any displayable error and turns it into the role's failure reason.
pub trait Ctx<T> {
    fn ctx(self, what: &str) -> Res<T>;
}

impl<T, E: std::fmt::Display> Ctx<T> for Result<T, E> {
    fn ctx(self, what: &str) -> Res<T> {
        self.map_err(|e| format!("{what}: {e}"))
    }
}

/// What a role that ran to the end reports. A role that cannot run returns `Err(reason)` instead.
pub struct Outcome {
    pub pass: bool,
    pub load_ms: u128,
    pub first_ms: u128,
    pub detail: String,
    pub reason: String,
}

impl Outcome {
    pub fn new(load_ms: u128, first_ms: u128) -> Self {
        Self {
            pass: false,
            load_ms,
            first_ms,
            detail: String::new(),
            reason: String::new(),
        }
    }
}

/// Settings every role reads from the command line.
pub struct Config {
    pub models_dir: PathBuf,
    pub reading_list: PathBuf,
    /// Also print the text a role produced, on a separate stderr line (`TEXT=...`).
    pub show_text: bool,
}

/// The result line on stderr (`--show-text` adds a `TEXT=` line). The sample pipe uses stdout.
pub fn result_line(role: &str, outcome: &Outcome) -> String {
    let result = if outcome.pass { "PASS" } else { "FAIL" };
    format!(
        "ROLE={role} RESULT={result} LOAD_MS={} FIRST_MS={} DETAIL={} REASON={}",
        outcome.load_ms,
        outcome.first_ms,
        one_line(&outcome.detail),
        one_line(&outcome.reason)
    )
}

pub fn failure_line(role: &str, reason: &str) -> String {
    let mut outcome = Outcome::new(0, 0);
    outcome.reason = reason.to_string();
    result_line(role, &outcome)
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

pub fn ms(since: Instant) -> u128 {
    since.elapsed().as_millis()
}

pub fn new_environment() -> Res<Environment> {
    ort::init().build().ctx("runtime init")
}

/// Loads one ONNX file with four intra-op threads. A missing file is a plain failure reason.
pub fn load_session(env: &Environment, path: &Path) -> Res<Session> {
    if !path.is_file() {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        return Err(format!(
            "model file missing: {}",
            name.unwrap_or_else(|| "?".to_string())
        ));
    }
    let mut builder = Session::builder(env)
        .ctx("session builder")?
        .with_intra_threads(4)
        .ctx("thread setting")?;
    builder
        .commit_from_file(path)
        .ctx(&format!("load {}", path.display()))
}

/// Reads a small text file of the model folder.
pub fn read_text(path: &Path) -> Res<String> {
    std::fs::read_to_string(path).ctx(&format!("read {}", path.display()))
}

/// Finds `"key": <integer>` in a flat JSON text. The configs of the models are flat; a full JSON
/// parser is not a dependency of this spike.
pub fn json_int(text: &str, key: &str) -> Res<i64> {
    let needle = format!("\"{key}\"");
    let at = text
        .find(&needle)
        .ok_or_else(|| format!("config key missing: {key}"))?;
    let rest = text[at + needle.len()..].trim_start();
    let rest = rest
        .strip_prefix(':')
        .ok_or_else(|| format!("config key malformed: {key}"))?
        .trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end]
        .parse::<i64>()
        .map_err(|_| format!("config value is not an integer: {key}"))
}

/// Like `json_int`, for a size: it must be between 1 and 65536.
pub fn json_usize(text: &str, key: &str) -> Res<usize> {
    let value = json_int(text, key)?;
    if (1..=65_536).contains(&value) {
        Ok(value as usize)
    } else {
        Err(format!("config value out of range: {key}"))
    }
}

/// The test sentence: row `vi_001` of the reading list, read at run time.
pub fn sentence(reading_list: &Path) -> Res<String> {
    let text = read_text(reading_list)?;
    for line in text.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() >= 4 && cells[1] == "vi_001" {
            return Ok(cells[3].to_string());
        }
    }
    Err("row vi_001 not found in the reading list".to_string())
}

/// Longest input a role reads: two minutes of 16 kHz mono f32 samples.
const MAX_INPUT_BYTES: u64 = 16_000 * 4 * 120;

/// Reads 16 kHz mono f32 little-endian samples from stdin until end of file (or the cap).
pub fn read_samples_from_stdin() -> Res<Vec<f32>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES)
        .read_to_end(&mut bytes)
        .ctx("read stdin")?;
    if bytes.is_empty() {
        return Err("no input".to_string());
    }
    Ok(f32s_from_le(&bytes))
}

/// Decodes f32 little-endian values; trailing bytes that do not fill a value are ignored.
pub fn f32s_from_le(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// Splits a tensor shape from the runtime into plain sizes.
pub fn dims(shape: &[i64]) -> Vec<usize> {
    shape.iter().map(|&d| d.max(0) as usize).collect()
}

/// Index of the largest value (first one on a tie).
pub fn argmax(values: &[f32]) -> usize {
    let mut best = 0;
    for (i, v) in values.iter().enumerate() {
        if *v > values[best] {
            best = i;
        }
    }
    best
}

/// Composes the text (NFC) with the tokenizer crate's normalizer.
pub fn nfc(text: &str) -> Res<String> {
    let mut normalized = NormalizedString::from(text);
    tokenizers::normalizers::NFC
        .normalize(&mut normalized)
        .map_err(|e| format!("normalize: {e}"))?;
    Ok(normalized.get().to_string())
}

/// Shape of the merged decoders of the translation and recognition models: 6 layers, 8 heads of
/// 64 values. A step without a cache still has to feed every past tensor, with length zero.
const DECODER_LAYERS: usize = 6;
const DECODER_HEADS: usize = 8;
const DECODER_HEAD_DIM: usize = 64;

/// Adds the empty past tensors a merged decoder asks for when its cache branch is off.
pub fn push_empty_past(inputs: &mut Vec<(String, DynValue)>) -> Res<()> {
    for layer in 0..DECODER_LAYERS {
        for part in ["decoder", "encoder"] {
            for kind in ["key", "value"] {
                inputs.push((
                    format!("past_key_values.{layer}.{part}.{kind}"),
                    Tensor::from_array((
                        [1usize, DECODER_HEADS, 0, DECODER_HEAD_DIM],
                        Vec::<f32>::new(),
                    ))
                    .ctx("empty past")?
                    .into_dyn(),
                ));
            }
        }
    }
    Ok(())
}
