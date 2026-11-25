//! Speech recognition with a model loaded in process.
//!
//! One call takes the samples of one speech span (0.1 s to 30 s, 16 kHz mono)
//! and returns the Vietnamese text of the whole span. A partial text is a
//! second call on a longer buffer; the caller owns that.

use crate::error::EngineError;
use crate::onnx_vad::file_name;
use crate::speech_features::N_MELS;
use crate::traits::Recognition;
use meeting_core::protocol::Language;
use std::path::Path;
use tokenizers::normalizers::NFC;
use tokenizers::{NormalizedString, Normalizer, Tokenizer};

/// Largest text file the adapter reads (config, tokenizer): a safety bound.
const MAX_TEXT_FILE_BYTES: u64 = 64 * 1024 * 1024;

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

fn config_error(what: &str, key: &str) -> EngineError {
    EngineError::Backend(format!("config.json {what}: {key}"))
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
