//! The text rules of the translation adapter: what goes into the model, what
//! comes out, and the one repair of the tokenizer file.

use crate::error::EngineError;
use crate::onnx_seq2seq::{compose, TOKENIZER_FILE};

/// The start of the error texts about the model; a text file error names the
/// file instead.
pub(crate) const LABEL: &str = "translation model";
/// The marks that end a sentence when white space follows them.
const SENTENCE_MARKS: [char; 4] = ['.', '?', '!', '\u{2026}'];
/// The two spellings, without white space, of a compiled normalizer that holds
/// nothing. The tokenizer library panics on it.
const EMPTY_COMPILED_NORMALIZERS: [&str; 2] = [
    r#"{"type":"Precompiled","precompiled_charsmap":null}"#,
    r#"{"precompiled_charsmap":null,"type":"Precompiled"}"#,
];

/// The input for the model: composed, trimmed, with a capital letter at each
/// sentence start (the model translates lower-case text badly). `None` when the
/// text holds no letter or digit: the model would invent a sentence for it.
pub(crate) fn prepare_input(text: &str) -> Result<Option<String>, EngineError> {
    let composed = compose(LABEL, text)?;
    let trimmed = composed.trim();
    if !trimmed.chars().any(char::is_alphanumeric) {
        return Ok(None);
    }
    Ok(Some(capitalise_sentence_starts(trimmed)))
}

/// Upper case for the first character of the text and of each sentence after a
/// mark and white space. Every other character is kept.
fn capitalise_sentence_starts(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut at_start = true;
    let mut after_mark = false;
    for c in text.chars() {
        if c.is_whitespace() {
            at_start |= after_mark;
            after_mark = false;
            result.push(c);
            continue;
        }
        if at_start {
            result.extend(c.to_uppercase());
        } else {
            result.push(c);
        }
        at_start = false;
        after_mark = SENTENCE_MARKS.contains(&c);
    }
    result
}

/// The tokenizer file as the tokenizer library can read it. When the value of the
/// top level key `normalizer` is the empty compiled normalizer, it becomes
/// `null`; the adapter composes the input itself. Another value of that key that
/// holds the word `Precompiled`, and an object there that does not close, are
/// errors. Every other text is returned unchanged. This is no guard against a
/// hostile file (a second key or an escaped spelling gets past it, and the
/// library may then panic): the caller must pass files that the model manager
/// has verified.
pub(crate) fn repair_tokenizer_text(text: &str) -> Result<String, EngineError> {
    let unsupported =
        || EngineError::Backend(format!("{TOKENIZER_FILE} has an unsupported normalizer"));
    let Some(start) = normalizer_value_start(text) else {
        return Ok(text.to_string());
    };
    let rest = &text[start..];
    let length = match rest.bytes().next() {
        Some(b'{') => container_length(rest).ok_or_else(unsupported)?,
        // A list or a string that does not close runs to the end of the text,
        // so the word check below reads all of it.
        Some(b'[') => container_length(rest).unwrap_or(rest.len()),
        Some(b'"') => string_end(rest.as_bytes(), 0).unwrap_or(rest.len()),
        _ => return Ok(text.to_string()),
    };
    let end = start + length;
    let value = &text[start..end];
    if !value.contains("Precompiled") {
        return Ok(text.to_string());
    }
    // Neither accepted spelling holds white space inside a string.
    let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    if !EMPTY_COMPILED_NORMALIZERS.contains(&compact.as_str()) {
        return Err(unsupported());
    }
    Ok(format!("{}null{}", &text[..start], &text[end..]))
}

/// The byte position just after the string that opens at `quote`.
fn string_end(bytes: &[u8], quote: usize) -> Option<usize> {
    let mut at = quote + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return Some(at + 1),
            _ => at += 1,
        }
    }
    None
}

/// Where the value of the top level key `normalizer` starts. A word of the same
/// spelling deeper in the file, such as a vocabulary entry, is not that key.
fn normalizer_value_start(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut at, mut depth) = (0_usize, 0_usize);
    while at < bytes.len() {
        match bytes[at] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            b'"' => {
                let end = string_end(bytes, at)?;
                if depth == 1 && &bytes[at..end] == b"\"normalizer\"" {
                    if let Some(value) = text[end..].trim_start().strip_prefix(':') {
                        return Some(text.len() - value.trim_start().len());
                    }
                }
                at = end;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// The length of the object or list that `text` starts with, or `None` if it
/// does not close.
fn container_length(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut at, mut depth) = (0_usize, 0_usize);
    while at < bytes.len() {
        match bytes[at] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            b'"' => {
                at = string_end(bytes, at)?;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// The output of one call from the decoded text: trimmed, and never empty.
pub(crate) fn build_translation(raw: &str) -> Result<String, EngineError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(EngineError::Backend(format!("{LABEL}: empty output")));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokenizers::Tokenizer;

    /// A word level tokenizer text with `MODEL` before or after the normalizer.
    /// One vocabulary entry is the word `normalizer`.
    const MODEL: &str = r#""model":{"type":"WordLevel","vocab":{"[UNK]":0,"normalizer":1,"a":2},"unk_token":"[UNK]"}"#;
    const HEAD: &str = r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],
"pre_tokenizer":{"type":"Whitespace"},"post_processor":null,"decoder":null,"#;

    fn normalizer_first(normalizer: &str) -> String {
        format!("{HEAD}\n\"normalizer\": {normalizer} ,\n{MODEL}}}")
    }

    fn model_first(normalizer: &str) -> String {
        format!("{HEAD}\n{MODEL},\n\"normalizer\":{normalizer}}}")
    }

    fn error_text<T: std::fmt::Debug>(result: Result<T, EngineError>) -> String {
        match result {
            Err(EngineError::Backend(text)) => text,
            other => format!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn input_text_is_composed_and_trimmed() {
        let composed = "T\u{1ec7} th\u{1ead}t";
        for variant in [
            // Fully decomposed.
            "  Te\u{323}\u{302} tha\u{323}\u{302}t \n",
            // Partly composed.
            "\tT\u{ea}\u{323} th\u{e2}\u{323}t  ",
            // The two marks in the other order.
            "\n Te\u{302}\u{323} tha\u{302}\u{323}t\t",
        ] {
            assert_eq!(
                prepare_input(variant),
                Ok(Some(composed.to_string())),
                "{variant:?}"
            );
        }
    }

    #[test]
    fn sentence_starts_get_a_capital_letter() {
        let input = "đây là câu một. ốc ở đâu? ừ! thế à… xem a.b và 2.5 phần.  \nạ. 3 con mèo.";
        let expected = "Đây là câu một. Ốc ở đâu? Ừ! Thế à… Xem a.b và 2.5 phần.  \nẠ. 3 con mèo.";
        assert_eq!(prepare_input(input), Ok(Some(expected.to_string())));
    }

    #[test]
    fn text_without_a_letter_or_digit_gives_no_input() {
        for empty in ["", "   ", ".", "?! \u{2026}"] {
            assert_eq!(prepare_input(empty), Ok(None), "{empty:?}");
        }
        assert_eq!(prepare_input("5"), Ok(Some("5".to_string())));
        assert_eq!(prepare_input("a"), Ok(Some("A".to_string())));
    }

    #[test]
    fn empty_precompiled_normalizer_is_removed() {
        for compiled in [
            r#"{"type":"Precompiled","precompiled_charsmap":null}"#,
            "{ \"precompiled_charsmap\" : null ,\n  \"type\" : \"Precompiled\" }",
        ] {
            for build in [normalizer_first, model_first] {
                let repaired = repair_tokenizer_text(&build(compiled));
                assert_eq!(repaired, Ok(build("null")), "{compiled}");
                let parsed = repaired.map(|text| text.parse::<Tokenizer>().is_ok());
                assert_eq!(parsed, Ok(true), "{compiled}");
            }
        }
    }

    #[test]
    fn other_precompiled_normalizer_is_refused() {
        let mut refused: Vec<String> = [
            r#"{"type":"Precompiled","precompiled_charsmap":"abc"}"#,
            r#"{"type":"Precompiled","precompiled_charsmap":null,"extra":1}"#,
            r#"{"type":"Sequence","normalizers":[{"type":"Precompiled","precompiled_charsmap":null}]}"#,
            // A value that is not an object.
            r#"[{"type":"Precompiled","precompiled_charsmap":null}]"#,
            r#""Precompiled""#,
        ]
        .iter()
        .map(|normalizer| normalizer_first(normalizer))
        .collect();
        // A compiled normalizer in a value that does not close: an object, a
        // list and a string.
        refused.extend(
            [
                r#"{"version":"1.0","normalizer":{"type":"Precompiled","precompiled_charsmap":null"#,
                r#"{"version":"1.0","normalizer":[{"type":"Precompiled","precompiled_charsmap":null}"#,
                r#"{"version":"1.0","normalizer":"Precompiled"#,
            ]
            .map(str::to_string),
        );
        // An object that does not close is refused without the word too.
        refused.push(r#"{"version":"1.0","normalizer":{"type":"NFC""#.to_string());
        for text in refused {
            let error = error_text(repair_tokenizer_text(&text));
            assert!(error.contains("tokenizer.json"), "{error} for {text}");
        }
    }

    #[test]
    fn other_tokenizer_text_is_unchanged() {
        let mut texts: Vec<String> = [
            "null",
            r#"{"type":"NFC"}"#,
            r#"{"type":"Sequence","normalizers":[{"type":"NFC"},{"type":"Lowercase"}]}"#,
        ]
        .iter()
        .map(|normalizer| normalizer_first(normalizer))
        .collect();
        // No normalizer key; the vocabulary entry of that name stays.
        texts.push(format!("{HEAD}\n{MODEL}}}"));
        // A list and a string that do not close and hold no compiled normalizer.
        texts.push(r#"{"version":"1.0","normalizer":[{"type":"NFC"}"#.to_string());
        texts.push(r#"{"version":"1.0","normalizer":"NFC"#.to_string());
        for text in texts {
            assert_eq!(repair_tokenizer_text(&text), Ok(text.clone()));
        }
    }

    #[test]
    fn output_text_is_trimmed_and_never_empty() {
        assert_eq!(
            build_translation("  Good morning.  "),
            Ok("Good morning.".to_string())
        );
        for empty in ["", "   "] {
            let error = error_text(build_translation(empty));
            assert!(error.contains("empty output"), "{error}");
        }
    }
}
