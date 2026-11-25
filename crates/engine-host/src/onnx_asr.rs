//! Speech recognition with a model loaded in process.
//!
//! One call takes the samples of one speech span (0.1 s to 30 s, 16 kHz mono)
//! and returns the Vietnamese text of the whole span. A partial text is a
//! second call on a longer buffer; the caller owns that. A span outside the
//! limits, a model that never writes its end token and a failed run are typed
//! errors, never a cut or a guess. The adapter keeps nothing of a call.
//!
//! The adapter does not check the size or the SHA-256 of the model files. The
//! caller must pass a folder that the model manager has verified in the same
//! run, before the call: a model that fails its checksum never runs
//! (guardrail 7). The check is owned by M1-04 and M1-42. The adapter opens no
//! connection and logs nothing. It allocates, so it runs on a worker thread,
//! never in an audio callback.
//!
//! The runtime writes its own log lines through `tracing`; those lines can hold
//! the model path and are not scrubbed. A log sink must filter the `ort` target.

use crate::error::EngineError;
use crate::onnx_vad::{file_name, scrub, shared_environment};
use crate::speech_features::{check_span, FeatureExtractor, FRAMES, N_MELS};
use crate::traits::{Engine, Recognition, Recognizer};
use meeting_core::policy::Provider;
use meeting_core::protocol::Language;
use ort::environment::Environment;
use ort::session::{Session, SessionInputValue, SessionOutputs};
use ort::value::{DynValue, Tensor};
use std::path::Path;
use tokenizers::normalizers::NFC;
use tokenizers::{NormalizedString, Normalizer, Tokenizer};

/// Largest text file the adapter reads (config, tokenizer): a safety bound.
const MAX_TEXT_FILE_BYTES: u64 = 64 * 1024 * 1024;

const CONFIG_FILE: &str = "config.json";
const TOKENIZER_FILE: &str = "tokenizer.json";
const ENCODER_FILE: &str = "encoder.onnx";
const DECODER_FILE: &str = "decoder.onnx";
/// Threads of each session: a starting value (measured in the spike); M1-26 may
/// tune it.
const INTRA_OP_THREADS: usize = 4;
/// Names of the prompt tokens after the start token, in order: language, task,
/// no time stamps.
const PROMPT_TOKENS: [&str; 3] = ["<|vi|>", "<|transcribe|>", "<|notimestamps|>"];
/// A size in the config must lie between 1 and this value.
const MAX_CONFIG_SIZE: i64 = 65_536;

/// Sizes of the decoder, read from the model config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecoderShape {
    pub(crate) layers: usize,
    pub(crate) heads: usize,
    pub(crate) head_dim: usize,
}

/// The recognition engine. Local: it never leaves this Mac.
pub struct OnnxRecognizer {
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
    features: FeatureExtractor,
    prompt: Vec<i64>,
    end_token: i64,
    shape: DecoderShape,
    slots: Vec<CacheSlot>,
    token_limit: usize,
    use_cache: bool,
    provider: Provider,
}

impl std::fmt::Debug for OnnxRecognizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxRecognizer")
            .field("token_limit", &self.token_limit)
            .finish_non_exhaustive()
    }
}

fn backend(what: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError::Backend(format!("recognition model: {what}: {error}"))
}

/// One session with `INTRA_OP_THREADS` threads. Runtime text is scrubbed of the
/// path.
fn load_session(environment: &Environment, path: &Path) -> Result<Session, EngineError> {
    let name = file_name(path);
    let fail = |what: &str, e: &dyn std::fmt::Display| backend(what, scrub(&e.to_string(), path));
    let mut builder = Session::builder(environment)
        .map_err(|e| fail("session builder", &e))?
        .with_intra_threads(INTRA_OP_THREADS)
        .map_err(|e| fail("thread setting", &e))?;
    builder
        .commit_from_file(path)
        .map_err(|e| fail(&format!("load {name}"), &e))
}

/// An id from the config: it must not be negative.
fn config_token(config: &str, key: &str) -> Result<i64, EngineError> {
    let value = config_int(config, key)?;
    if value >= 0 {
        Ok(value)
    } else {
        Err(config_error("value is out of range", key))
    }
}

/// Names of one tensor of the decoder cache. Only the decoder-side tensors grow
/// from step to step; the encoder-side ones are written once, by the first step.
struct CacheSlot {
    past: String,
    present: String,
    grows: bool,
}

fn cache_slots(layers: usize) -> Vec<CacheSlot> {
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

/// A failed run of the model or of a tensor. The text is fixed: it holds no
/// token id, no recognized text and no sample.
fn run_failed(what: &str) -> EngineError {
    EngineError::Backend(format!("recognition model: {what} failed"))
}

/// The scores of the last position of a decoder step (output `logits`, shape
/// `[1, positions, vocabulary]`).
fn last_logits(outputs: &SessionOutputs<'_>) -> Result<Vec<f32>, EngineError> {
    let (shape, logits) = outputs
        .get("logits")
        .ok_or_else(|| run_failed("decoder scores"))?
        .try_extract_tensor::<f32>()
        .map_err(|_| run_failed("decoder scores"))?;
    let vocabulary = shape
        .last()
        .and_then(|&size| usize::try_from(size).ok())
        .filter(|&size| size > 0 && size <= logits.len())
        .ok_or_else(|| run_failed("decoder scores"))?;
    Ok(logits[logits.len() - vocabulary..].to_vec())
}

fn take(outputs: &mut SessionOutputs<'_>, name: &str) -> Result<DynValue, EngineError> {
    outputs
        .remove(name)
        .ok_or_else(|| run_failed("decoder output"))
}

/// One decoding of one span. The cache values stay in the session's memory and
/// are fed back as views; nothing is copied between steps.
struct DecoderRun<'a> {
    session: &'a mut Session,
    slots: &'a [CacheSlot],
    hidden: DynValue,
    /// Past tensors of length zero, for a step that has no cache.
    empty: Vec<DynValue>,
    /// The cache after the first step; empty before it.
    cache: Vec<DynValue>,
}

impl DecoderRun<'_> {
    /// Run the decoder over `ids` and return the scores of the next position.
    /// With `with_cache` the step feeds the kept cache and `ids` is the last
    /// token only; without it the step gets empty past tensors. `keep` stores
    /// what the step wrote into the cache.
    fn step(&mut self, ids: &[i64], with_cache: bool, keep: bool) -> Result<Vec<f32>, EngineError> {
        let ids_tensor = Tensor::from_array(([1_usize, ids.len()], ids.to_vec()))
            .map_err(|_| run_failed("token tensor"))?;
        let flag = Tensor::from_array(([1_usize], vec![with_cache]))
            .map_err(|_| run_failed("flag tensor"))?;
        let past = if with_cache { &self.cache } else { &self.empty };
        let mut inputs: Vec<(&str, SessionInputValue<'_>)> = vec![
            ("input_ids", ids_tensor.into()),
            ("encoder_hidden_states", (&self.hidden).into()),
            ("use_cache_branch", flag.into()),
        ];
        for (slot, value) in self.slots.iter().zip(past) {
            inputs.push((slot.past.as_str(), value.into()));
        }
        let mut outputs = self
            .session
            .run(inputs)
            .map_err(|_| run_failed("decoder run"))?;
        let logits = last_logits(&outputs)?;
        if keep && self.cache.is_empty() {
            for slot in self.slots {
                self.cache.push(take(&mut outputs, &slot.present)?);
            }
        } else if keep {
            for (slot, cell) in self.slots.iter().zip(self.cache.iter_mut()) {
                if slot.grows {
                    *cell = take(&mut outputs, &slot.present)?;
                }
            }
        }
        Ok(logits)
    }
}

impl OnnxRecognizer {
    /// Load the model from the folder `dir`: `config.json`, `tokenizer.json`,
    /// `encoder.onnx` and `decoder.onnx`. They are checked in this order, and
    /// an error names the file or the key and never the folder. This call does
    /// not verify the files: the caller passes a folder that the model manager
    /// has already checked (see the module doc). It reads the files, so it runs
    /// on a worker thread, never in an audio callback.
    pub fn open(dir: &Path) -> Result<Self, EngineError> {
        let config = read_text_file(&dir.join(CONFIG_FILE))?;
        let shape = decoder_shape(&config)?;
        let start_token = config_token(&config, "decoder_start_token_id")?;
        let end_token = config_token(&config, "eos_token_id")?;
        let positions = config_size(&config, "max_target_positions")?;
        let tokenizer = read_text_file(&dir.join(TOKENIZER_FILE))?
            .parse::<Tokenizer>()
            .map_err(|e| EngineError::Backend(format!("{TOKENIZER_FILE}: {e}")))?;
        let prompt = prompt_ids(&tokenizer, start_token)?;
        let token_limit = token_limit(positions, prompt.len())?;
        for name in [ENCODER_FILE, DECODER_FILE] {
            if !dir.join(name).is_file() {
                return Err(EngineError::Backend(format!(
                    "recognition model file is missing: {name}"
                )));
            }
        }
        let environment = shared_environment()?;
        let encoder = load_session(&environment, &dir.join(ENCODER_FILE))?;
        let decoder = load_session(&environment, &dir.join(DECODER_FILE))?;
        Ok(Self {
            encoder,
            decoder,
            tokenizer,
            features: FeatureExtractor::new(),
            prompt,
            end_token,
            slots: cache_slots(shape.layers),
            shape,
            token_limit,
            use_cache: true,
            provider: Provider::local("onnx-asr"),
        })
    }

    /// The encoder over the features of the span; the output stays in the
    /// runtime's memory.
    fn encode(&mut self, samples: &[f32]) -> Result<DynValue, EngineError> {
        let input = Tensor::from_array(([1_usize, N_MELS, FRAMES], self.features.extract(samples)))
            .map_err(|_| run_failed("feature tensor"))?;
        let mut outputs = self
            .encoder
            .run(vec![("input_features", input)])
            .map_err(|_| run_failed("encoder run"))?;
        take(&mut outputs, "last_hidden_state")
    }

    /// Greedy decoding of the text ids from the encoder output.
    fn decode(&mut self, hidden: DynValue) -> Result<Vec<i64>, EngineError> {
        let empty = self
            .slots
            .iter()
            .map(|_| {
                Tensor::from_array((
                    [1_usize, self.shape.heads, 0, self.shape.head_dim],
                    Vec::<f32>::new(),
                ))
                .map(Tensor::into_dyn)
                .map_err(|_| run_failed("empty past tensor"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut run = DecoderRun {
            session: &mut self.decoder,
            slots: &self.slots,
            hidden,
            empty,
            cache: Vec::new(),
        };
        let (prompt, use_cache) = (&self.prompt, self.use_cache);
        let mut sequence = prompt.clone();
        greedy_decode(self.end_token, self.token_limit, |previous| {
            match (previous, use_cache) {
                (None, _) => run.step(prompt, false, use_cache),
                (Some(last), true) => run.step(&[last], true, true),
                (Some(last), false) => {
                    sequence.push(last);
                    run.step(&sequence, false, false)
                }
            }
        })
    }

    /// Run the decoder over the whole prefix at every step, without its cache.
    /// Slower; the text is the same (checked against the cached run).
    #[must_use]
    pub fn without_decoder_cache(mut self) -> Self {
        self.use_cache = false;
        self
    }
}

impl Engine for OnnxRecognizer {
    fn provider(&self) -> &Provider {
        &self.provider
    }
}

impl Recognizer for OnnxRecognizer {
    fn recognize(&mut self, samples: &[f32]) -> Result<Recognition, EngineError> {
        check_span(samples)?;
        let hidden = self.encode(samples)?;
        let ids = self.decode(hidden)?;
        build_recognition(&ids_to_text(&self.tokenizer, &ids)?)
    }
}

fn config_error(what: &str, key: &str) -> EngineError {
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
fn config_size(text: &str, key: &str) -> Result<usize, EngineError> {
    let value = config_int(text, key)?;
    if (1..=MAX_CONFIG_SIZE).contains(&value) {
        usize::try_from(value).map_err(|_| config_error("value is out of range", key))
    } else {
        Err(config_error("value is out of range", key))
    }
}

/// Layers, heads and head size of the decoder. The features make 80 mel bands,
/// so a model with another number of bands is refused.
pub(crate) fn decoder_shape(config: &str) -> Result<DecoderShape, EngineError> {
    let layers = config_size(config, "decoder_layers")?;
    let heads = config_size(config, "decoder_attention_heads")?;
    let width = config_size(config, "d_model")?;
    if config_int(config, "num_mel_bins")? != N_MELS as i64 {
        return Err(config_error("value is not supported", "num_mel_bins"));
    }
    if width % heads != 0 {
        return Err(config_error(
            "value does not divide into the heads",
            "d_model",
        ));
    }
    Ok(DecoderShape {
        layers,
        heads,
        head_dim: width / heads,
    })
}

/// The largest number of text tokens: the decoder positions minus the prompt.
pub(crate) fn token_limit(
    max_target_positions: usize,
    prompt_len: usize,
) -> Result<usize, EngineError> {
    if max_target_positions > prompt_len {
        Ok(max_target_positions - prompt_len)
    } else {
        Err(config_error("value is too small", "max_target_positions"))
    }
}

pub(crate) fn check_text_size(length: u64) -> Result<(), EngineError> {
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
    if let Err(EngineError::Backend(text)) = check_text_size(length) {
        return Err(EngineError::Backend(format!("{name}: {text}")));
    }
    std::fs::read_to_string(path).map_err(unreadable)
}

/// The start token, then the ids of the language, task and time stamp tokens
/// looked up by name.
pub(crate) fn prompt_ids(tokenizer: &Tokenizer, start_id: i64) -> Result<Vec<i64>, EngineError> {
    let mut ids = vec![start_id];
    for name in PROMPT_TOKENS {
        let id = tokenizer
            .token_to_id(name)
            .ok_or_else(|| EngineError::Backend(format!("tokenizer.json has no token {name}")))?;
        ids.push(i64::from(id));
    }
    Ok(ids)
}

/// Text of generated ids; special tokens are skipped.
pub(crate) fn ids_to_text(tokenizer: &Tokenizer, ids: &[i64]) -> Result<String, EngineError> {
    let failed = || EngineError::Backend("recognition model: token decode failed".into());
    let ids: Vec<u32> = ids
        .iter()
        .map(|&id| u32::try_from(id).map_err(|_| failed()))
        .collect::<Result<_, _>>()?;
    tokenizer.decode(&ids, true).map_err(|_| failed())
}

/// The next token: the largest score among the text tokens and the end token
/// (every id above the end token is a control or time stamp token). The lowest
/// id wins a tie. A score that is not finite is an error.
pub(crate) fn pick_next_token(logits: &[f32], end_token: i64) -> Result<i64, EngineError> {
    if logits.iter().any(|score| !score.is_finite()) {
        return Err(EngineError::Backend(
            "recognition model: a score is not a finite number".into(),
        ));
    }
    let mut best: Option<usize> = None;
    for (id, score) in logits.iter().enumerate() {
        if i64::try_from(id).map_or(true, |id| id > end_token) {
            break;
        }
        if best.is_none_or(|b| *score > logits[b]) {
            best = Some(id);
        }
    }
    best.and_then(|id| i64::try_from(id).ok())
        .ok_or_else(|| EngineError::Backend("recognition model: no score to pick".into()))
}

/// Greedy decoding. `step` gets `None` for the prompt and the last token after
/// it, and returns the scores of the next position. The end token is not
/// returned. At most `token_limit` text tokens are allowed: the end token must
/// come on call `token_limit + 1` at the latest, else the result is an error
/// that holds no token id.
pub(crate) fn greedy_decode(
    end_token: i64,
    token_limit: usize,
    mut step: impl FnMut(Option<i64>) -> Result<Vec<f32>, EngineError>,
) -> Result<Vec<i64>, EngineError> {
    let mut ids = Vec::new();
    let mut previous = None;
    loop {
        let next = pick_next_token(&step(previous)?, end_token)?;
        if next == end_token {
            return Ok(ids);
        }
        if ids.len() == token_limit {
            return Err(EngineError::Backend(
                "recognition model: no end token within the token limit".into(),
            ));
        }
        ids.push(next);
        previous = Some(next);
    }
}

/// The text composed (NFC), trimmed, and marked Vietnamese: this model writes
/// Vietnamese only.
pub(crate) fn build_recognition(raw: &str) -> Result<Recognition, EngineError> {
    let mut normalized = NormalizedString::from(raw);
    NFC.normalize(&mut normalized)
        .map_err(|_| EngineError::Backend("recognition model: text compose failed".into()))?;
    Ok(Recognition {
        text: normalized.get().trim().to_string(),
        detected: Language::Vi,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use meeting_core::protocol::Language;

    /// A byte-level tokenizer with six text tokens and the three prompt tokens.
    const SMALL_TOKENIZER: &str = r#"{"version":"1.0","truncation":null,"padding":null,
"added_tokens":[
{"id":6,"content":"<|vi|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true},
{"id":7,"content":"<|transcribe|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true},
{"id":8,"content":"<|notimestamps|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true}],
"normalizer":null,
"pre_tokenizer":{"type":"ByteLevel","add_prefix_space":false,"trim_offsets":true,"use_regex":true},
"post_processor":null,
"decoder":{"type":"ByteLevel","add_prefix_space":true,"trim_offsets":true,"use_regex":true},
"model":{"type":"BPE","dropout":null,"unk_token":null,"continuing_subword_prefix":null,
"end_of_word_suffix":null,"fuse_unk":false,"byte_fallback":false,"ignore_merges":false,
"vocab":{"a":0,"b":1,"c":2,"Ġ":3,"d":4,"e":5},"merges":[]}}"#;

    fn config(layers: u32, heads: u32, d_model: u32, mel_bins: u32, positions: u32) -> String {
        format!(
            r#"{{"d_model": {d_model}, "decoder_attention_heads": {heads}, "decoder_layers": {layers},
"decoder_start_token_id": 50258, "eos_token_id": 50257, "max_target_positions": {positions},
"num_mel_bins": {mel_bins}}}"#
        )
    }

    fn small_tokenizer() -> Result<Tokenizer, String> {
        SMALL_TOKENIZER
            .parse::<Tokenizer>()
            .map_err(|e| e.to_string())
    }

    /// A step that returns, call after call, 12 scores with the largest at the
    /// next id of `script`, and counts its calls.
    fn scripted<'a>(
        script: &'a [usize],
        calls: &'a mut usize,
    ) -> impl FnMut(Option<i64>) -> Result<Vec<f32>, EngineError> + 'a {
        move |_| {
            let id = script
                .get(*calls)
                .copied()
                .ok_or_else(|| EngineError::Backend("script is too short".into()))?;
            *calls += 1;
            let mut scores = vec![0.0_f32; 12];
            scores[id] = 1.0;
            Ok(scores)
        }
    }

    #[test]
    fn end_token_stops_the_decoding_and_is_not_returned() {
        let mut calls = 0;
        let ids = greedy_decode(9, 10, scripted(&[3, 4, 9], &mut calls));
        assert_eq!(ids, Ok(vec![3, 4]));
        assert_eq!(calls, 3);
    }

    #[test]
    fn token_limit_is_exact_and_a_missing_end_token_is_a_typed_error() {
        let mut calls = 0;
        let ids = greedy_decode(9, 5, scripted(&[1, 2, 3, 4, 5, 9], &mut calls));
        assert_eq!(ids, Ok(vec![1, 2, 3, 4, 5]));
        assert_eq!(calls, 6);

        let mut calls = 0;
        let result = greedy_decode(9, 5, scripted(&[1, 2, 3, 4, 5, 6, 7, 8], &mut calls));
        assert_eq!(calls, 6);
        let text = match result {
            Err(EngineError::Backend(text)) => text,
            other => format!("unexpected: {other:?}"),
        };
        assert!(text.contains("end token"), "{text}");
        assert!(!text.contains(['1', '2', '3', '4', '5', '6']), "{text}");
    }

    #[test]
    fn next_token_ignores_ids_above_the_end_token_and_refuses_non_finite_scores() {
        let mut scores = vec![0.0_f32; 12];
        scores[11] = 5.0;
        scores[4] = 1.0;
        assert_eq!(pick_next_token(&scores, 9), Ok(4));
        // The end token itself may win.
        scores[9] = 2.0;
        assert_eq!(pick_next_token(&scores, 9), Ok(9));
        // On a tie the lower id wins.
        let tie = [0.0_f32, 3.0, 1.0, 3.0, 0.0];
        assert_eq!(pick_next_token(&tie, 4), Ok(1));
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut broken = vec![0.0_f32; 12];
            broken[10] = bad;
            assert!(
                matches!(pick_next_token(&broken, 9), Err(EngineError::Backend(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn decoder_shape_follows_the_config_and_refuses_odd_sizes() {
        let base = decoder_shape(&config(6, 8, 512, 80, 448));
        assert_eq!(
            base,
            Ok(DecoderShape {
                layers: 6,
                heads: 8,
                head_dim: 64
            })
        );
        let larger = decoder_shape(&config(12, 12, 768, 80, 448));
        assert_eq!(
            larger,
            Ok(DecoderShape {
                layers: 12,
                heads: 12,
                head_dim: 64
            })
        );
        let no_layers =
            decoder_shape(r#"{"d_model": 512, "decoder_attention_heads": 8, "num_mel_bins": 80}"#);
        let text = match no_layers {
            Err(EngineError::Backend(text)) => text,
            other => format!("unexpected: {other:?}"),
        };
        assert!(text.contains("decoder_layers"), "{text}");
        for odd in [config(6, 8, 500, 80, 448), config(6, 8, 512, 128, 448)] {
            assert!(
                matches!(decoder_shape(&odd), Err(EngineError::Backend(_))),
                "{odd}"
            );
        }
        assert_eq!(token_limit(448, 4), Ok(444));
        for positions in [0_usize, 3, 4] {
            assert!(
                matches!(token_limit(positions, 4), Err(EngineError::Backend(_))),
                "{positions}"
            );
        }
    }

    #[test]
    fn recognition_text_is_composed_trimmed_and_marked_vietnamese() {
        let decomposed = "  cha\u{0300}o e\u{0323}\u{0302} ";
        let recognition = build_recognition(decomposed);
        assert_eq!(
            recognition,
            Ok(Recognition {
                text: "chào ệ".to_string(),
                detected: Language::Vi,
            })
        );
        let empty = build_recognition("  ");
        assert_eq!(
            empty,
            Ok(Recognition {
                text: String::new(),
                detected: Language::Vi,
            })
        );
    }

    #[test]
    fn prompt_holds_start_language_task_and_no_timestamps_in_order() -> Result<(), String> {
        let tokenizer = small_tokenizer()?;
        assert_eq!(prompt_ids(&tokenizer, 3), Ok(vec![3, 6, 7, 8]));
        Ok(())
    }

    #[test]
    fn missing_prompt_token_is_an_error_that_names_it() -> Result<(), String> {
        let without = SMALL_TOKENIZER.replace("<|vi|>", "<|xx|>");
        let tokenizer = without.parse::<Tokenizer>().map_err(|e| e.to_string())?;
        let text = match prompt_ids(&tokenizer, 3) {
            Err(EngineError::Backend(text)) => text,
            other => format!("unexpected: {other:?}"),
        };
        assert!(text.contains("<|vi|>"), "{text}");
        Ok(())
    }

    #[test]
    fn special_tokens_are_skipped_when_ids_become_text() -> Result<(), String> {
        let tokenizer = small_tokenizer()?;
        assert_eq!(
            ids_to_text(&tokenizer, &[0, 1, 6, 3, 2]),
            Ok("ab c".to_string())
        );
        Ok(())
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
