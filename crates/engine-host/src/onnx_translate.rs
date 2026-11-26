//! Vietnamese to English translation with a model loaded in process.
//!
//! One call takes one Vietnamese text and returns its English text. The input
//! is composed and gets a capital letter at each sentence start; a text without
//! a letter or digit gives an empty text and the model does not run. An input
//! over the positions of the model, a block of tokens that keeps repeating, a
//! model that never writes its end token and a failed run are typed errors,
//! never a cut or a guess. The adapter keeps nothing of a call.
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
use crate::onnx_seq2seq::{
    cache_slots, config_size, config_token, decode_ids, decoder_sizes, greedy_decode, layer_shape,
    load_session, read_text_file, require_model_files, run_failed, take, CacheSlot, DecoderRun,
    DecoderShape, CONFIG_FILE, DECODER_FILE, ENCODER_FILE, TOKENIZER_FILE,
};
use crate::onnx_vad::shared_environment;
use crate::traits::{Engine, Translator};
use crate::translation_text::{build_translation, prepare_input, repair_tokenizer_text};
use meeting_core::policy::Provider;
use meeting_core::protocol::Language;
use ort::session::{Session, SessionInputValue};
use ort::value::{DynValue, Tensor};
use std::path::Path;
use tokenizers::Tokenizer;

/// The start of every error text of this model.
pub(crate) const LABEL: &str = "translation model";

/// Copies of one block in a row that stop the decoding: a starting value
/// (measured with a probe); M0-06 may change it.
const REPEAT_COPIES: usize = 6;
/// The longest block, in tokens, that counts as a repeat: a starting value.
const REPEAT_MAX_BLOCK_TOKENS: usize = 4;
/// Output tokens allowed per input token, so a model that never ends is stopped
/// early: a starting value (the measured ratio stayed under 2).
const OUTPUT_TOKENS_PER_INPUT_TOKEN: usize = 3;
/// Tokens added to that bound, so that a very short input has room: a starting
/// value.
const OUTPUT_TOKEN_MARGIN: usize = 10;

/// Sizes and special ids of the model, read from its config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TranslationSizes {
    shape: DecoderShape,
    positions: usize,
    start: i64,
    padding: i64,
    end: i64,
}

fn check_direction(from: Language, to: Language) -> Result<(), EngineError> {
    if matches!((from, to), (Language::Vi, Language::En)) {
        Ok(())
    } else {
        Err(EngineError::Backend(format!(
            "{LABEL}: only Vietnamese to English is supported"
        )))
    }
}

/// An input longer than the positions of the model is refused, never cut.
fn check_input_length(tokens: usize, max_positions: usize) -> Result<(), EngineError> {
    if tokens > max_positions {
        Err(EngineError::Backend(format!(
            "{LABEL}: the input is too long"
        )))
    } else {
        Ok(())
    }
}

/// The largest number of output tokens: it follows the input and stays under the
/// positions of the model (the start token takes one). Reaching it is an error,
/// never a cut text.
fn output_token_limit(max_positions: usize, input_tokens: usize) -> usize {
    let bound = input_tokens * OUTPUT_TOKENS_PER_INPUT_TOKEN + OUTPUT_TOKEN_MARGIN;
    bound.min(max_positions.saturating_sub(1))
}

/// The next token: the largest score, never the padding or the start id. The
/// lowest id wins a tie. A score that is not finite is an error.
fn pick_translation_token(scores: &[f32], padding: i64, start: i64) -> Result<i64, EngineError> {
    if scores.iter().any(|score| !score.is_finite()) {
        return Err(EngineError::Backend(format!(
            "{LABEL}: a score is not a finite number"
        )));
    }
    let mut best: Option<(i64, f32)> = None;
    for (id, &score) in (0_i64..).zip(scores) {
        if id != padding && id != start && best.is_none_or(|(_, top)| score > top) {
            best = Some((id, score));
        }
    }
    best.map(|(id, _)| id)
        .ok_or_else(|| EngineError::Backend(format!("{LABEL}: no score to pick")))
}

/// Whether the ids end in `REPEAT_COPIES` copies of one short block.
fn ends_in_repeat(ids: &[i64]) -> bool {
    (1..=REPEAT_MAX_BLOCK_TOKENS).any(|block| {
        ids.len()
            .checked_sub(block * REPEAT_COPIES)
            .is_some_and(|from| {
                let tail = &ids[from..];
                tail.chunks(block).all(|copy| copy == &tail[..block])
            })
    })
}

/// The token choice of this model: pick, then refuse a token that completes a
/// repeat. The error holds no id and no text.
fn translation_choice(
    scores: &[f32],
    ids: &[i64],
    sizes: &TranslationSizes,
) -> Result<i64, EngineError> {
    let next = pick_translation_token(scores, sizes.padding, sizes.start)?;
    if next != sizes.end {
        let window = REPEAT_COPIES * REPEAT_MAX_BLOCK_TOKENS - 1;
        let mut tail = ids[ids.len().saturating_sub(window)..].to_vec();
        tail.push(next);
        if ends_in_repeat(&tail) {
            return Err(EngineError::Backend(format!("{LABEL}: the output repeats")));
        }
    }
    Ok(next)
}

fn translation_sizes(config: &str) -> Result<TranslationSizes, EngineError> {
    Ok(TranslationSizes {
        shape: layer_shape(decoder_sizes(config)?)?,
        positions: config_size(config, "max_position_embeddings")?,
        start: config_token(config, "decoder_start_token_id")?,
        padding: config_token(config, "pad_token_id")?,
        end: config_token(config, "eos_token_id")?,
    })
}

/// The translation engine. Local: it never leaves this Mac.
pub struct OnnxTranslator {
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
    sizes: TranslationSizes,
    slots: Vec<CacheSlot>,
    use_cache: bool,
    provider: Provider,
}

impl std::fmt::Debug for OnnxTranslator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxTranslator")
            .field("positions", &self.sizes.positions)
            .finish_non_exhaustive()
    }
}

impl OnnxTranslator {
    /// Load the model from the folder `dir`: `config.json`, `tokenizer.json`,
    /// `encoder.onnx` and `decoder.onnx`. They are checked in this order, and
    /// an error names the file or the key and never the folder. This call does
    /// not verify the files: the caller passes a folder that the model manager
    /// has already checked (see the module doc). It reads the files, so it runs
    /// on a worker thread, never in an audio callback.
    pub fn open(dir: &Path) -> Result<Self, EngineError> {
        let sizes = translation_sizes(&read_text_file(&dir.join(CONFIG_FILE))?)?;
        let tokenizer = repair_tokenizer_text(&read_text_file(&dir.join(TOKENIZER_FILE))?)?
            .parse::<Tokenizer>()
            .map_err(|_| {
                EngineError::Backend(format!("{TOKENIZER_FILE} is not a valid tokenizer"))
            })?;
        require_model_files(LABEL, dir)?;
        let environment = shared_environment()
            .map_err(|_| EngineError::Backend(format!("{LABEL}: runtime start failed")))?;
        let encoder = load_session(LABEL, &environment, &dir.join(ENCODER_FILE))?;
        let decoder = load_session(LABEL, &environment, &dir.join(DECODER_FILE))?;
        Ok(Self {
            encoder,
            decoder,
            tokenizer,
            slots: cache_slots(sizes.shape.layers),
            sizes,
            use_cache: true,
            provider: Provider::local("onnx-translate"),
        })
    }

    /// Run the decoder over the whole prefix at every step, without its cache.
    /// Slower; the text is the same (checked against the cached run).
    #[must_use]
    pub fn without_decoder_cache(mut self) -> Self {
        self.use_cache = false;
        self
    }

    /// The ids of the input; the tokenizer adds the end token.
    fn input_ids(&self, input: &str) -> Result<Vec<i64>, EngineError> {
        let encoding = self
            .tokenizer
            .encode(input, true)
            .map_err(|_| run_failed(LABEL, "token encode"))?;
        Ok(encoding.get_ids().iter().map(|&id| i64::from(id)).collect())
    }

    /// The encoder over the input ids. Returns its output, which stays in the
    /// runtime's memory, and the mask that the decoder needs again.
    fn encode(&mut self, ids: Vec<i64>) -> Result<(DynValue, DynValue), EngineError> {
        let length = ids.len();
        let ids = Tensor::from_array(([1_usize, length], ids))
            .map_err(|_| run_failed(LABEL, "token tensor"))?;
        let mask = Tensor::from_array(([1_usize, length], vec![1_i64; length]))
            .map_err(|_| run_failed(LABEL, "mask tensor"))?;
        let inputs: Vec<(&str, SessionInputValue<'_>)> = vec![
            ("input_ids", ids.into()),
            ("attention_mask", (&mask).into()),
        ];
        let mut outputs = self
            .encoder
            .run(inputs)
            .map_err(|_| run_failed(LABEL, "encoder run"))?;
        let hidden = take(LABEL, "encoder output", &mut outputs, "last_hidden_state")?;
        Ok((hidden, mask.into_dyn()))
    }

    /// Greedy decoding of the output ids from the encoder output.
    fn decode(
        &mut self,
        hidden: DynValue,
        mask: DynValue,
        token_limit: usize,
    ) -> Result<Vec<i64>, EngineError> {
        let sizes = self.sizes;
        let run = DecoderRun::new(
            LABEL,
            &mut self.decoder,
            &self.slots,
            sizes.shape,
            vec![
                ("encoder_hidden_states", hidden),
                ("encoder_attention_mask", mask),
            ],
        )?;
        greedy_decode(
            LABEL,
            sizes.end,
            token_limit,
            |scores, ids| translation_choice(scores, ids, &sizes),
            run.into_step(vec![sizes.start], self.use_cache),
        )
    }
}

impl Engine for OnnxTranslator {
    fn provider(&self) -> &Provider {
        &self.provider
    }
}

impl Translator for OnnxTranslator {
    fn translate(
        &mut self,
        text: &str,
        from: Language,
        to: Language,
    ) -> Result<String, EngineError> {
        check_direction(from, to)?;
        let Some(input) = prepare_input(text)? else {
            return Ok(String::new());
        };
        let ids = self.input_ids(&input)?;
        check_input_length(ids.len(), self.sizes.positions)?;
        let limit = output_token_limit(self.sizes.positions, ids.len());
        let (hidden, mask) = self.encode(ids)?;
        let output = self.decode(hidden, mask, limit)?;
        build_translation(&decode_ids(LABEL, &self.tokenizer, &output)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onnx_seq2seq::greedy_decode;

    /// End id 0, padding id 10, start id 11, and 12 scores per step.
    const SIZES: TranslationSizes = TranslationSizes {
        shape: DecoderShape {
            layers: 1,
            heads: 1,
            head_dim: 1,
        },
        positions: 512,
        start: 11,
        padding: 10,
        end: 0,
    };

    fn error_text<T: std::fmt::Debug>(result: Result<T, EngineError>) -> String {
        match result {
            Err(EngineError::Backend(text)) => text,
            other => format!("unexpected: {other:?}"),
        }
    }

    /// Decode with the translation choice. Call after call, `script` names the
    /// id with the largest score. The second value is the number of calls.
    fn decode(
        limit: usize,
        script: impl Fn(usize) -> usize,
    ) -> (Result<Vec<i64>, EngineError>, usize) {
        let mut calls = 0;
        let result = greedy_decode(
            LABEL,
            SIZES.end,
            limit,
            |scores, ids| translation_choice(scores, ids, &SIZES),
            |_| {
                let mut scores = vec![0.0_f32; 12];
                scores[script(calls)] = 1.0;
                calls += 1;
                Ok(scores)
            },
        );
        (result, calls)
    }

    #[test]
    fn only_vietnamese_to_english_is_accepted() {
        assert_eq!(check_direction(Language::Vi, Language::En), Ok(()));
        for (from, to) in [
            (Language::En, Language::Vi),
            (Language::Vi, Language::Vi),
            (Language::En, Language::En),
        ] {
            let error = error_text(check_direction(from, to));
            assert!(error.contains("only Vietnamese to English"), "{error}");
        }
    }

    #[test]
    fn input_over_the_position_limit_is_refused() {
        assert_eq!(check_input_length(512, 512), Ok(()));
        assert_eq!(
            error_text(check_input_length(513, 512)),
            "translation model: the input is too long"
        );
    }

    #[test]
    fn output_token_limit_follows_the_input_and_is_capped() {
        for (input, limit) in [
            (2, 16),
            (17, 61),
            (166, 508),
            (167, 511),
            (168, 511),
            (512, 511),
        ] {
            assert_eq!(output_token_limit(512, input), limit, "{input} tokens");
        }
    }

    #[test]
    fn next_token_skips_padding_and_refuses_non_finite_scores() {
        let (padding, start) = (10_i64, 11_i64);
        let mut scores = vec![0.0_f32; 12];
        scores[4] = 1.0;
        scores[10] = 9.0;
        assert_eq!(pick_translation_token(&scores, padding, start), Ok(4));
        scores[10] = 0.0;
        scores[11] = 9.0;
        assert_eq!(pick_translation_token(&scores, padding, start), Ok(4));
        // On a tie the lower id wins.
        scores[7] = 1.0;
        assert_eq!(pick_translation_token(&scores, padding, start), Ok(4));
        // The end token can win.
        scores[0] = 2.0;
        assert_eq!(pick_translation_token(&scores, padding, start), Ok(0));
        for (at, bad) in [(10, f32::NAN), (11, f32::INFINITY), (3, f32::NEG_INFINITY)] {
            let mut broken = vec![0.0_f32; 12];
            broken[at] = bad;
            let result = pick_translation_token(&broken, padding, start);
            assert!(matches!(result, Err(EngineError::Backend(_))), "{bad}");
        }
    }

    #[test]
    fn six_copies_of_a_short_block_are_a_repeat() {
        let copies = |block: &[i64], count: usize| {
            let mut ids = vec![100_i64];
            for _ in 0..count {
                ids.extend_from_slice(block);
            }
            ids
        };
        for block in [&[1_i64][..], &[1, 2], &[1, 2, 3], &[1, 2, 3, 4]] {
            assert!(ends_in_repeat(&copies(block, 6)), "{block:?}");
            assert!(!ends_in_repeat(&copies(block, 5)), "{block:?}");
        }
        assert!(!ends_in_repeat(&copies(&[1, 2, 3, 4, 5], 6)));
        assert!(!ends_in_repeat(&[7, 1, 7, 2, 7, 3, 7, 4, 7, 5, 7]));
        assert!(!ends_in_repeat(&[]));
    }

    #[test]
    fn repeating_output_stops_the_decoding_with_a_typed_error() {
        let block = |call: usize| if call.is_multiple_of(2) { 3 } else { 4 };
        let (result, calls) = decode(100, block);
        assert_eq!(error_text(result), "translation model: the output repeats");
        assert_eq!(calls, 12);

        let (result, calls) = decode(100, |call| if call < 10 { block(call) } else { 0 });
        assert_eq!(result, Ok(vec![3, 4, 3, 4, 3, 4, 3, 4, 3, 4]));
        assert_eq!(calls, 11);
    }

    #[test]
    fn translation_stops_at_the_token_limit_with_a_typed_error() {
        let (result, calls) = decode(5, |call| if call < 5 { call + 3 } else { 0 });
        assert_eq!(result, Ok(vec![3, 4, 5, 6, 7]));
        assert_eq!(calls, 6);

        let (result, calls) = decode(5, |call| call + 1);
        assert_eq!(
            error_text(result),
            "translation model: no end token within the token limit"
        );
        assert_eq!(calls, 6);
    }

    #[test]
    fn translation_sizes_come_from_the_config() {
        const CONFIG: &str = r#"{"d_model": 512, "decoder_attention_heads": 8,
"decoder_layers": 6, "decoder_start_token_id": 9, "eos_token_id": 0,
"max_length": 100, "max_position_embeddings": 512, "pad_token_id": 8}"#;
        assert_eq!(
            translation_sizes(CONFIG),
            Ok(TranslationSizes {
                shape: DecoderShape {
                    layers: 6,
                    heads: 8,
                    head_dim: 64
                },
                positions: 512,
                start: 9,
                padding: 8,
                end: 0,
            })
        );
        for key in ["max_position_embeddings", "pad_token_id"] {
            let without = CONFIG.replace(key, "other_key");
            let error = error_text(translation_sizes(&without));
            assert!(error.contains(key), "{error}");
        }
    }
}
