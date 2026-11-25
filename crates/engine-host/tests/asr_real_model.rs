//! Recognition with the real model and a synthetic speech file.
//!
//! The model and the audio stay outside git. `INTERLO_MODELS_DIR` names the
//! folder that holds `M1-35/asr/` and `M1-36/vi_001.wav`. When the variable is
//! not set, each real-model test prints one SKIP line and passes. When it is
//! set and a file is missing, the test fails and names the file. The word
//! match tests at the end need no model.

use tokenizers::normalizers::NFC;
use tokenizers::{NormalizedString, Normalizer};

/// Words that must match for a pass: 13 of the 15 words of the sentence.
const PASS_WORDS: usize = 13;

/// Lower-cased, composed words of a text; every character that is not a
/// letter, a digit or white space is removed.
fn words(text: &str) -> Vec<String> {
    let mut normalized = NormalizedString::from(text.to_lowercase());
    if NFC.normalize(&mut normalized).is_err() {
        return Vec::new();
    }
    let cleaned: String = normalized
        .get()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    cleaned.split_whitespace().map(str::to_string).collect()
}

/// Expected words that the recognized text holds. A recognized word is used
/// once: a word counts when an equal recognized word is left that no earlier
/// expected word used.
fn words_correct(expected: &str, got: &str) -> usize {
    let mut left = words(got);
    let mut correct = 0;
    for word in words(expected) {
        if let Some(at) = left.iter().position(|w| *w == word) {
            left.swap_remove(at);
            correct += 1;
        }
    }
    correct
}

fn meets_pass_rule(correct: usize) -> bool {
    correct >= PASS_WORDS
}

#[test]
fn word_match_counts_each_expected_word_once() {
    assert_eq!(words_correct("a b c a", "a b c"), 3);
    assert_eq!(words_correct("a b c a", "a a b c"), 4);
    assert_eq!(words_correct("a a b c", "a b c"), 3);
    assert_eq!(words_correct("Xin, CHÀO!", "xin chào"), 2);
    // A composed and a decomposed spelling are the same word.
    assert_eq!(words_correct("chào", "cha\u{0300}o"), 1);
}

#[test]
fn thirteen_of_fifteen_words_pass_and_twelve_fail() {
    let expected: Vec<String> = (1..=15).map(|n| format!("w{n}")).collect();
    let expected = expected.join(" ");
    let recognized = |count: usize| -> String {
        (1..=count)
            .map(|n| format!("w{n}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(words_correct(&expected, &recognized(13)), 13);
    assert!(meets_pass_rule(words_correct(&expected, &recognized(13))));
    assert_eq!(words_correct(&expected, &recognized(12)), 12);
    assert!(!meets_pass_rule(words_correct(&expected, &recognized(12))));
}
