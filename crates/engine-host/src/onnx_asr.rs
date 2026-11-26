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
//! The tokenizer library writes the characters of a text that it handles through
//! `log` at trace level: a log sink must also cap the `tokenizers` target.

use crate::error::EngineError;
use crate::onnx_seq2seq::{
    self as seq2seq, best_score, cache_slots, config_error, config_int, config_size, config_token,
    decoder_sizes, layer_shape, load_sessions, parse_tokenizer, read_text_file, take, CacheSlot,
    DecoderRun, DecoderShape, CONFIG_FILE, TOKENIZER_FILE,
};
use crate::speech_features::{check_span, FeatureExtractor, FRAMES, N_MELS};
use crate::traits::{Engine, Recognition, Recognizer};
use meeting_core::policy::Provider;
use meeting_core::protocol::Language;
use ort::session::Session;
use ort::value::{DynValue, Tensor};
use std::path::Path;
use tokenizers::Tokenizer;

/// The start of the error texts about the model; a text file error names the
/// file instead.
const LABEL: &str = "recognition model";
/// Names of the prompt tokens after the start token, in order: language, task,
/// no time stamps.
const PROMPT_TOKENS: [&str; 3] = ["<|vi|>", "<|transcribe|>", "<|notimestamps|>"];
/// Sample rate of a span.
const SAMPLE_RATE_HZ: usize = 16_000;
/// Text tokens per second of speech that bound the decoding of a span. Starting
/// value: the real model gave at most 9.33 per second on the test sentence (14
/// tokens in the first 1.5 s), and this is more than 2 times that. A measurement on
/// more speakers (M0-05) may replace it.
const TOKENS_PER_SECOND: usize = 20;
/// Tokens added to the bound, so that a very short span still has room. Starting
/// value: no measurement; it is about 1 s of speech at the rate above.
const TOKEN_MARGIN: usize = 8;

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

/// A failed run of the model or of a tensor. The text is fixed: it holds no
/// token id, no recognized text and no sample.
fn run_failed(what: &str) -> EngineError {
    seq2seq::run_failed(LABEL, what)
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
        let tokenizer = parse_tokenizer(&read_text_file(&dir.join(TOKENIZER_FILE))?)?;
        let prompt = prompt_ids(&tokenizer, start_token)?;
        let token_limit = token_limit(positions, prompt.len())?;
        let (encoder, decoder) = load_sessions(LABEL, dir)?;
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
        // The recognition error texts are fixed, so this encoder output is
        // reported as a decoder output.
        take(LABEL, "decoder output", &mut outputs, "last_hidden_state")
    }

    /// Greedy decoding of the text ids from the encoder output.
    fn decode(&mut self, hidden: DynValue, token_limit: usize) -> Result<Vec<i64>, EngineError> {
        let run = DecoderRun::new(
            LABEL,
            &mut self.decoder,
            &self.slots,
            self.shape,
            vec![("encoder_hidden_states", hidden)],
        )?;
        let step = run.into_step(self.prompt.clone(), self.use_cache);
        greedy_decode(self.end_token, token_limit, step)
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
        let ids = self.decode(hidden, span_token_limit(self.token_limit, samples.len()))?;
        build_recognition(&seq2seq::decode_ids(LABEL, &self.tokenizer, &ids)?)
    }
}

/// Layers, heads and head size of the decoder. The features make 80 mel bands,
/// so a model with another number of bands is refused.
fn decoder_shape(config: &str) -> Result<DecoderShape, EngineError> {
    let sizes = decoder_sizes(config)?;
    if config_int(config, "num_mel_bins")? != N_MELS as i64 {
        return Err(config_error("value is not supported", "num_mel_bins"));
    }
    layer_shape(sizes)
}

/// The largest number of text tokens: the decoder positions minus the prompt.
fn token_limit(max_target_positions: usize, prompt_len: usize) -> Result<usize, EngineError> {
    if max_target_positions > prompt_len {
        Ok(max_target_positions - prompt_len)
    } else {
        Err(config_error("value is too small", "max_target_positions"))
    }
}

/// The token limit for one span: a short span cannot hold the text of a long
/// one, so a model that never ends is stopped early. A span of about 17.4 s or
/// longer keeps the limit of the model. Reaching the limit is still an error, never a cut text.
fn span_token_limit(model_limit: usize, span_samples: usize) -> usize {
    let spoken = (span_samples * TOKENS_PER_SECOND).div_ceil(SAMPLE_RATE_HZ);
    model_limit.min(spoken + TOKEN_MARGIN)
}

/// The start token, then the ids of the language, task and time stamp tokens
/// looked up by name.
fn prompt_ids(tokenizer: &Tokenizer, start_id: i64) -> Result<Vec<i64>, EngineError> {
    let mut ids = vec![start_id];
    for name in PROMPT_TOKENS {
        let id = tokenizer
            .token_to_id(name)
            .ok_or_else(|| EngineError::Backend(format!("tokenizer.json has no token {name}")))?;
        ids.push(i64::from(id));
    }
    Ok(ids)
}

/// The next token: the largest score among the text tokens and the end token
/// (every id above the end token is a control or time stamp token). The lowest
/// id wins a tie. A score that is not finite is an error.
fn pick_next_token(logits: &[f32], end_token: i64) -> Result<i64, EngineError> {
    best_score(LABEL, logits, |id| id <= end_token)
}

/// The shared greedy loop with the token choice of this model.
fn greedy_decode(
    end_token: i64,
    token_limit: usize,
    step: impl FnMut(Option<i64>) -> Result<Vec<f32>, EngineError>,
) -> Result<Vec<i64>, EngineError> {
    seq2seq::greedy_decode(
        LABEL,
        end_token,
        token_limit,
        |scores, _| pick_next_token(scores, end_token),
        step,
    )
}

/// The text composed (NFC), trimmed, and marked Vietnamese: this model writes
/// Vietnamese only.
fn build_recognition(raw: &str) -> Result<Recognition, EngineError> {
    Ok(Recognition {
        text: seq2seq::compose(LABEL, raw)?.trim().to_string(),
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
    fn span_token_limit_follows_the_span_length_and_is_capped() {
        // ceil(20 * seconds) + 8 for short spans.
        assert_eq!(span_token_limit(444, 1_600), 10);
        assert_eq!(span_token_limit(444, 16_000), 28);
        assert_eq!(span_token_limit(444, 24_000), 38);
        // The last span length below the model limit, and the first at it.
        assert_eq!(span_token_limit(444, 348_000), 443);
        assert_eq!(span_token_limit(444, 348_001), 444);
        // A 30 s span keeps the model limit.
        assert_eq!(span_token_limit(444, 480_000), 444);
        // A smaller model limit always wins.
        assert_eq!(span_token_limit(100, 480_000), 100);
        assert_eq!(span_token_limit(5, 1_600), 5);
    }

    #[test]
    fn a_model_that_never_ends_stops_at_the_bounded_limit_for_short_and_long_spans() {
        for (span_samples, expected_limit) in [(16_000_usize, 28_usize), (480_000, 444)] {
            let limit = span_token_limit(444, span_samples);
            assert_eq!(limit, expected_limit, "span of {span_samples} samples");
            let mut calls = 0;
            let result = greedy_decode(9, limit, |_| {
                calls += 1;
                let mut scores = vec![0.0_f32; 12];
                scores[3] = 1.0;
                Ok(scores)
            });
            assert!(matches!(result, Err(EngineError::Backend(_))), "{result:?}");
            // The end token is asked for on call `limit + 1` at the latest.
            assert_eq!(calls, limit + 1, "span of {span_samples} samples");
        }
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
            seq2seq::decode_ids(LABEL, &tokenizer, &[0, 1, 6, 3, 2]),
            Ok("ab c".to_string())
        );
        Ok(())
    }
}
