//! The parts that the two encoder and decoder adapters share: the size-checked
//! text file reader, the flat config reader, the tokenizer parser, the session
//! loader, the decoder cache loop, the token pick, the greedy decoding loop and
//! the text composing step.
//!
//! An adapter passes its label, which starts the text of every error from the
//! model. No error holds a folder path, a token id, a text or a sample.

use crate::error::EngineError;
use crate::onnx_vad::{file_name, scrub, shared_environment};
use ort::environment::Environment;
use ort::session::{Session, SessionInputValue, SessionOutputs};
use ort::value::{DynValue, Tensor};
use std::path::Path;
use tokenizers::normalizers::NFC;
use tokenizers::{NormalizedString, Normalizer, Tokenizer};

/// Largest text file an adapter reads (config, tokenizer): a safety bound.
const MAX_TEXT_FILE_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) const CONFIG_FILE: &str = "config.json";
pub(crate) const TOKENIZER_FILE: &str = "tokenizer.json";
pub(crate) const ENCODER_FILE: &str = "encoder.onnx";
pub(crate) const DECODER_FILE: &str = "decoder.onnx";
/// Threads of each session: a starting value (measured in the spike); M1-26 may
/// tune it.
const INTRA_OP_THREADS: usize = 4;
/// A size in the config must lie between 1 and this value.
const MAX_CONFIG_VALUE: i64 = 65_536;

fn backend(label: &str, what: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError::Backend(format!("{label}: {what}: {error}"))
}

/// A failed run of the model or of a tensor. The text is fixed: it holds no
/// token id, no text and no sample.
pub(crate) fn run_failed(label: &str, what: &str) -> EngineError {
    EngineError::Backend(format!("{label}: {what} failed"))
}

pub(crate) fn config_error(what: &str, key: &str) -> EngineError {
    EngineError::Backend(format!("{CONFIG_FILE} {what}: {key}"))
}

/// The integer after `"key":` in a flat JSON text. The model config is flat; a
/// JSON parser is not a dependency of this crate.
pub(crate) fn config_int(text: &str, key: &str) -> Result<i64, EngineError> {
    let needle = format!("\"{key}\"");
    let at = text
        .find(&needle)
        .ok_or_else(|| config_error("has no key", key))?;
    let rest = text[at + needle.len()..].trim_start();
    let rest = rest
        .strip_prefix(':')
        .ok_or_else(|| config_error("key is malformed", key))?
        .trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end]
        .parse::<i64>()
        .map_err(|_| config_error("value is not an integer", key))
}

/// Like `config_int`, for a size: it must lie between 1 and 65536.
pub(crate) fn config_size(text: &str, key: &str) -> Result<usize, EngineError> {
    let value = config_int(text, key)?;
    if (1..=MAX_CONFIG_VALUE).contains(&value) {
        usize::try_from(value).map_err(|_| config_error("value is out of range", key))
    } else {
        Err(config_error("value is out of range", key))
    }
}

/// An id from the config: it must not be negative.
pub(crate) fn config_token(config: &str, key: &str) -> Result<i64, EngineError> {
    let value = config_int(config, key)?;
    if value >= 0 {
        Ok(value)
    } else {
        Err(config_error("value is out of range", key))
    }
}

/// Layers, heads and model width of the decoder, as the config gives them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DecoderSizes {
    layers: usize,
    heads: usize,
    width: usize,
}

pub(crate) fn decoder_sizes(config: &str) -> Result<DecoderSizes, EngineError> {
    Ok(DecoderSizes {
        layers: config_size(config, "decoder_layers")?,
        heads: config_size(config, "decoder_attention_heads")?,
        width: config_size(config, "d_model")?,
    })
}

/// Sizes of the decoder cache tensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecoderShape {
    pub(crate) layers: usize,
    pub(crate) heads: usize,
    pub(crate) head_dim: usize,
}

/// The shape for `sizes`: the width must divide into the heads.
pub(crate) fn layer_shape(sizes: DecoderSizes) -> Result<DecoderShape, EngineError> {
    if !sizes.width.is_multiple_of(sizes.heads) {
        return Err(config_error(
            "value does not divide into the heads",
            "d_model",
        ));
    }
    Ok(DecoderShape {
        layers: sizes.layers,
        heads: sizes.heads,
        head_dim: sizes.width / sizes.heads,
    })
}

fn check_text_size(length: u64) -> Result<(), EngineError> {
    if length > MAX_TEXT_FILE_BYTES {
        Err(EngineError::Backend(format!(
            "a text file is larger than {MAX_TEXT_FILE_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}

/// The only place that reads a file: the size is checked before the read, and
/// an error names the file and not its folder.
pub(crate) fn read_text_file(path: &Path) -> Result<String, EngineError> {
    let name = file_name(path);
    let unreadable = |e: std::io::Error| EngineError::Backend(format!("cannot read {name}: {e}"));
    let length = std::fs::metadata(path).map_err(unreadable)?.len();
    if check_text_size(length).is_err() {
        return Err(EngineError::Backend(format!(
            "{name} is larger than {MAX_TEXT_FILE_BYTES} bytes"
        )));
    }
    std::fs::read_to_string(path).map_err(unreadable)
}

/// The tokenizer of a tokenizer file text; the error names the file and holds
/// nothing of it.
pub(crate) fn parse_tokenizer(text: &str) -> Result<Tokenizer, EngineError> {
    text.parse::<Tokenizer>()
        .map_err(|_| EngineError::Backend(format!("{TOKENIZER_FILE} is not a valid tokenizer")))
}

/// The encoder and decoder sessions of the model in `dir`. Both files must be
/// present before the runtime starts.
pub(crate) fn load_sessions(label: &str, dir: &Path) -> Result<(Session, Session), EngineError> {
    require_model_files(label, dir)?;
    let environment = shared_environment()
        .map_err(|_| EngineError::Backend(format!("{label}: runtime start failed")))?;
    let encoder = load_session(label, &environment, &dir.join(ENCODER_FILE))?;
    let decoder = load_session(label, &environment, &dir.join(DECODER_FILE))?;
    Ok((encoder, decoder))
}

/// Both model files must be present in `dir`; an error names the missing one.
fn require_model_files(label: &str, dir: &Path) -> Result<(), EngineError> {
    for name in [ENCODER_FILE, DECODER_FILE] {
        if !dir.join(name).is_file() {
            return Err(EngineError::Backend(format!(
                "{label} file is missing: {name}"
            )));
        }
    }
    Ok(())
}

/// One session with `INTRA_OP_THREADS` threads. Runtime text is scrubbed of the
/// path.
fn load_session(
    label: &str,
    environment: &Environment,
    path: &Path,
) -> Result<Session, EngineError> {
    let name = file_name(path);
    let fail =
        |what: &str, e: &dyn std::fmt::Display| backend(label, what, scrub(&e.to_string(), path));
    let mut builder = Session::builder(environment)
        .map_err(|e| fail("session builder", &e))?
        .with_intra_threads(INTRA_OP_THREADS)
        .map_err(|e| fail("thread setting", &e))?;
    builder
        .commit_from_file(path)
        .map_err(|e| fail(&format!("load {name}"), &e))
}

/// Names of one tensor of the decoder cache. Only the decoder-side tensors grow
/// from step to step; the encoder-side ones are written once, by the first step.
pub(crate) struct CacheSlot {
    past: String,
    present: String,
    grows: bool,
}

pub(crate) fn cache_slots(layers: usize) -> Vec<CacheSlot> {
    let mut slots = Vec::with_capacity(layers * 4);
    for layer in 0..layers {
        for part in ["decoder", "encoder"] {
            for kind in ["key", "value"] {
                slots.push(CacheSlot {
                    past: format!("past_key_values.{layer}.{part}.{kind}"),
                    present: format!("present.{layer}.{part}.{kind}"),
                    grows: part == "decoder",
                });
            }
        }
    }
    slots
}

/// The scores of the last position of a decoder step (output `logits`, shape
/// `[1, positions, vocabulary]`).
fn last_logits(label: &str, outputs: &SessionOutputs<'_>) -> Result<Vec<f32>, EngineError> {
    let failed = || run_failed(label, "decoder scores");
    let (shape, logits) = outputs
        .get("logits")
        .ok_or_else(failed)?
        .try_extract_tensor::<f32>()
        .map_err(|_| failed())?;
    let vocabulary = shape
        .last()
        .and_then(|&size| usize::try_from(size).ok())
        .filter(|&size| size > 0 && size <= logits.len())
        .ok_or_else(failed)?;
    Ok(logits[logits.len() - vocabulary..].to_vec())
}

/// Move the output `name` out of `outputs`; `what` names it in the error.
pub(crate) fn take(
    label: &str,
    what: &str,
    outputs: &mut SessionOutputs<'_>,
    name: &str,
) -> Result<DynValue, EngineError> {
    outputs.remove(name).ok_or_else(|| run_failed(label, what))
}

/// What one decoder step feeds and stores.
#[derive(Debug, PartialEq, Eq)]
enum Pass<'p> {
    /// The whole prefix with empty past tensors; nothing is stored.
    Whole(&'p [i64]),
    /// The prompt with empty past tensors; the cache is stored.
    Prime(&'p [i64]),
    /// One token with the stored cache; the decoder-side cache is replaced.
    Cached(i64),
}

/// The pass of one step. `previous` is `None` for the first step. Without the
/// cache, `prefix` is the prompt and every token after it.
fn pass_of(previous: Option<i64>, use_cache: bool, prefix: &[i64]) -> Pass<'_> {
    match (previous, use_cache) {
        (None, true) => Pass::Prime(prefix),
        (Some(last), true) => Pass::Cached(last),
        (_, false) => Pass::Whole(prefix),
    }
}

/// One decoding of one input. The cache values stay in the session's memory and
/// are fed back as views; nothing is copied between steps.
pub(crate) struct DecoderRun<'a> {
    label: &'static str,
    session: &'a mut Session,
    slots: &'a [CacheSlot],
    /// Inputs that every step gets unchanged, by name.
    fixed: Vec<(&'static str, DynValue)>,
    /// Past tensors of length zero, for a step that has no cache.
    empty: Vec<DynValue>,
    /// The cache after the first step; empty before it.
    cache: Vec<DynValue>,
}

impl<'a> DecoderRun<'a> {
    pub(crate) fn new(
        label: &'static str,
        session: &'a mut Session,
        slots: &'a [CacheSlot],
        shape: DecoderShape,
        fixed: Vec<(&'static str, DynValue)>,
    ) -> Result<Self, EngineError> {
        let empty = slots
            .iter()
            .map(|_| {
                Tensor::from_array(([1_usize, shape.heads, 0, shape.head_dim], Vec::<f32>::new()))
                    .map(Tensor::into_dyn)
                    .map_err(|_| run_failed(label, "empty past tensor"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            label,
            session,
            slots,
            fixed,
            empty,
            cache: Vec::new(),
        })
    }

    /// The step function of a greedy loop: the prompt first, then the last token
    /// with the cache, or the whole prefix again when the cache is off.
    pub(crate) fn into_step(
        mut self,
        prompt: Vec<i64>,
        use_cache: bool,
    ) -> impl FnMut(Option<i64>) -> Result<Vec<f32>, EngineError> + 'a {
        // The whole prefix; it grows only when the cache is off.
        let mut prefix = prompt;
        move |previous| {
            if let (Some(last), false) = (previous, use_cache) {
                prefix.push(last);
            }
            self.step(&pass_of(previous, use_cache, &prefix))
        }
    }

    /// Run the decoder for one `pass` and return the scores of the next position.
    fn step(&mut self, pass: &Pass<'_>) -> Result<Vec<f32>, EngineError> {
        let label = self.label;
        let (ids, with_cache, keep) = match pass {
            Pass::Whole(ids) => (ids.to_vec(), false, false),
            Pass::Prime(ids) => (ids.to_vec(), false, true),
            Pass::Cached(last) => (vec![*last], true, true),
        };
        let ids_tensor = Tensor::from_array(([1_usize, ids.len()], ids))
            .map_err(|_| run_failed(label, "token tensor"))?;
        let flag = Tensor::from_array(([1_usize], vec![with_cache]))
            .map_err(|_| run_failed(label, "flag tensor"))?;
        let past = if with_cache { &self.cache } else { &self.empty };
        let mut inputs: Vec<(&str, SessionInputValue<'_>)> = vec![
            ("input_ids", ids_tensor.into()),
            ("use_cache_branch", flag.into()),
        ];
        for (name, value) in &self.fixed {
            inputs.push((*name, value.into()));
        }
        for (slot, value) in self.slots.iter().zip(past) {
            inputs.push((slot.past.as_str(), value.into()));
        }
        let mut outputs = self
            .session
            .run(inputs)
            .map_err(|_| run_failed(label, "decoder run"))?;
        let logits = last_logits(label, &outputs)?;
        if keep && self.cache.is_empty() {
            for slot in self.slots {
                self.cache
                    .push(take(label, "decoder output", &mut outputs, &slot.present)?);
            }
        } else if keep {
            for (slot, cell) in self.slots.iter().zip(self.cache.iter_mut()) {
                if slot.grows {
                    *cell = take(label, "decoder output", &mut outputs, &slot.present)?;
                }
            }
        }
        Ok(logits)
    }
}

/// The id with the largest score among the ids that `allowed` accepts. The
/// lowest id wins a tie. A score that is not finite is an error, also at an id
/// that is not allowed.
pub(crate) fn best_score(
    label: &str,
    scores: &[f32],
    allowed: impl Fn(i64) -> bool,
) -> Result<i64, EngineError> {
    if scores.iter().any(|score| !score.is_finite()) {
        return Err(EngineError::Backend(format!(
            "{label}: a score is not a finite number"
        )));
    }
    let mut best: Option<(i64, f32)> = None;
    for (id, &score) in (0_i64..).zip(scores) {
        if allowed(id) && best.is_none_or(|(_, top)| score > top) {
            best = Some((id, score));
        }
    }
    best.map(|(id, _)| id)
        .ok_or_else(|| EngineError::Backend(format!("{label}: no score to pick")))
}

/// Greedy decoding. `step` gets `None` for the prompt and the last token after
/// it, and returns the scores of the next position; `choose` picks the next
/// token from those scores and the ids so far. The end token is not returned.
/// At most `token_limit` text tokens are allowed: the end token must come on
/// call `token_limit + 1` at the latest, else the result is an error that holds
/// no token id.
pub(crate) fn greedy_decode(
    label: &str,
    end_token: i64,
    token_limit: usize,
    mut choose: impl FnMut(&[f32], &[i64]) -> Result<i64, EngineError>,
    mut step: impl FnMut(Option<i64>) -> Result<Vec<f32>, EngineError>,
) -> Result<Vec<i64>, EngineError> {
    let mut ids = Vec::new();
    let mut previous = None;
    loop {
        let scores = step(previous)?;
        let next = choose(&scores, &ids)?;
        if next == end_token {
            return Ok(ids);
        }
        if ids.len() == token_limit {
            return Err(EngineError::Backend(format!(
                "{label}: no end token within the token limit"
            )));
        }
        ids.push(next);
        previous = Some(next);
    }
}

/// Text of generated ids; special tokens are skipped.
pub(crate) fn decode_ids(
    label: &str,
    tokenizer: &Tokenizer,
    ids: &[i64],
) -> Result<String, EngineError> {
    let failed = || run_failed(label, "token decode");
    let ids: Vec<u32> = ids
        .iter()
        .map(|&id| u32::try_from(id).map_err(|_| failed()))
        .collect::<Result<_, _>>()?;
    tokenizer.decode(&ids, true).map_err(|_| failed())
}

/// The text in its composed form (NFC).
pub(crate) fn compose(label: &str, text: &str) -> Result<String, EngineError> {
    let mut normalized = NormalizedString::from(text);
    NFC.normalize(&mut normalized)
        .map_err(|_| run_failed(label, "text compose"))?;
    Ok(normalized.get().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_pass_follows_the_cache_setting() {
        let prefix = [7, 8];
        assert_eq!(pass_of(None, true, &prefix), Pass::Prime(&prefix));
        assert_eq!(pass_of(Some(9), true, &prefix), Pass::Cached(9));
        assert_eq!(pass_of(None, false, &prefix), Pass::Whole(&prefix));
        assert_eq!(pass_of(Some(9), false, &prefix), Pass::Whole(&prefix));
    }

    #[test]
    fn text_file_over_the_limit_is_refused() {
        assert_eq!(check_text_size(67_108_864), Ok(()));
        assert!(matches!(
            check_text_size(67_108_865),
            Err(EngineError::Backend(_))
        ));
    }
}
