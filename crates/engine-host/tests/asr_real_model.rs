//! Recognition with the real model and a synthetic speech file.
//!
//! The model and the audio stay outside git. `INTERLO_MODELS_DIR` names the
//! folder that holds `M1-35/asr/` and `M1-36/vi_001.wav`. When the variable is
//! not set, each real-model test prints one SKIP line and passes. When it is
//! set and a file is missing, the test fails and names the file. The word
//! match tests need no model.

use engine_host::{EngineError, EngineRegistry, OnnxRecognizer, Recognizer};
use meeting_core::policy::PrivacyPolicy;
use meeting_core::protocol::Language;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use tokenizers::normalizers::NFC;
use tokenizers::{NormalizedString, Normalizer};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SKIP_LINE: &str = "SKIP real-model test: INTERLO_MODELS_DIR is not set";
/// Files of the model folder that the adapter reads.
const MODEL_FILES: [&str; 4] = [
    "encoder.onnx",
    "decoder.onnx",
    "tokenizer.json",
    "config.json",
];
const SAMPLE_RATE_HZ: usize = 16_000;
/// The 5 s span of the speed report.
const SPEED_SPAN_SAMPLES: usize = 80_000;
const SPEED_CALLS: usize = 5;

/// Real-model tests run one at a time: each loads about 2.5 GB, and a parallel
/// test would distort the speed report.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The models folder from the variable value, or the skip line.
fn resolve(value: Option<&str>) -> Result<PathBuf, &'static str> {
    value.map(PathBuf::from).ok_or(SKIP_LINE)
}

/// A file under the models folder; a missing file is an error that names it.
fn require(dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = dir.join(relative);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("missing file for the real-model test: {relative}"))
    }
}

/// The models folder, or `None` after printing the skip line.
fn models_dir() -> Option<PathBuf> {
    let value = std::env::var("INTERLO_MODELS_DIR").ok();
    match resolve(value.as_deref()) {
        Ok(dir) => Some(dir),
        Err(line) => {
            println!("{line}");
            None
        }
    }
}

/// Mono 32-bit float WAV at 16 kHz to samples. Anything else is an error.
fn read_wav(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read wav: {e}"))?;
    let word = |at: usize| -> Result<u32, String> {
        let part = bytes.get(at..at + 4).ok_or("wav is cut short")?;
        Ok(u32::from_le_bytes([part[0], part[1], part[2], part[3]]))
    };
    let half = |at: usize| -> Result<u16, String> {
        let part = bytes.get(at..at + 2).ok_or("wav is cut short")?;
        Ok(u16::from_le_bytes([part[0], part[1]]))
    };
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("not a RIFF WAVE file".into());
    }
    let mut at = 12;
    let mut format_ok = false;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = word(at + 4)? as usize;
        let body = at + 8;
        if id == b"fmt " {
            format_ok = half(body)? == 3
                && half(body + 2)? == 1
                && word(body + 4)? == 16_000
                && half(body + 14)? == 32;
        } else if id == b"data" {
            if !format_ok {
                return Err("wav must be mono 32-bit float at 16 kHz".into());
            }
            let data = bytes
                .get(body..body + size)
                .ok_or("wav data is cut short")?;
            return Ok(data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect());
        }
        at = body + size + (size & 1);
    }
    Err("wav has no data chunk".into())
}

/// The sentence of row `vi_001` of the reading list, read at run time.
fn sentence() -> Result<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/m0/reading-list.md");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read reading list: {e}"))?;
    for line in text.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() >= 4 && cells[1] == "vi_001" {
            return Ok(cells[3].to_string());
        }
    }
    Err("row vi_001 not found in the reading list".into())
}

struct Fixture {
    model_dir: PathBuf,
    samples: Vec<f32>,
    expected: String,
}

/// The model folder, the speech samples and the sentence, or `None` when the
/// test is skipped.
fn fixture() -> Result<Option<Fixture>, String> {
    let Some(dir) = models_dir() else {
        return Ok(None);
    };
    let model_dir = dir.join("M1-35/asr");
    for name in MODEL_FILES {
        require(&dir, &format!("M1-35/asr/{name}"))?;
    }
    let samples = read_wav(&require(&dir, "M1-36/vi_001.wav")?)?;
    Ok(Some(Fixture {
        model_dir,
        samples,
        expected: sentence()?,
    }))
}

fn backend_text(error: Option<EngineError>) -> String {
    match error {
        Some(EngineError::Backend(text)) => text,
        other => format!("not a Backend error: {other:?}"),
    }
}

#[test]
fn real_model_tests_skip_with_a_message_when_the_folder_is_not_set() {
    assert_eq!(resolve(None), Err(SKIP_LINE));
    assert_eq!(resolve(Some("/models")), Ok(PathBuf::from("/models")));
}

#[test]
fn real_model_test_fails_when_a_file_is_missing() {
    let error = require(Path::new("/nonexistent-models-folder"), "M1-36/vi_001.wav");
    assert_eq!(
        error,
        Err("missing file for the real-model test: M1-36/vi_001.wav".to_string())
    );
}

#[test]
fn speech_file_is_recognized_with_at_least_13_of_15_words() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let _one = one_at_a_time();
    let mut recognizer = OnnxRecognizer::open(&f.model_dir)?;
    let recognition = recognizer.recognize(&f.samples)?;
    let correct = words_correct(&f.expected, &recognition.text);
    println!(
        "text: {}\nwords correct: {correct} of {}",
        recognition.text,
        words(&f.expected).len()
    );
    assert_eq!(recognition.detected, Language::Vi);
    assert!(!recognition.text.is_empty());
    assert!(meets_pass_rule(correct), "{correct} words correct");
    Ok(())
}

#[test]
fn spans_outside_the_limits_are_refused_with_a_backend_error() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let _one = one_at_a_time();
    let mut recognizer = OnnxRecognizer::open(&f.model_dir)?;
    for length in [0_usize, 1_599, 480_001] {
        let text = backend_text(recognizer.recognize(&vec![0.0; length]).err());
        assert!(!text.starts_with("not a Backend error"), "{length}: {text}");
    }
    Ok(())
}

#[test]
fn calls_after_refused_spans_and_repeated_calls_give_the_text_of_a_fresh_adapter() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let _one = one_at_a_time();
    let mut used = OnnxRecognizer::open(&f.model_dir)?;
    for length in [0_usize, 1_599, 480_001] {
        assert!(used.recognize(&vec![0.0; length]).is_err());
    }
    let first = used.recognize(&f.samples)?;
    let second = used.recognize(&f.samples)?;
    let fresh = OnnxRecognizer::open(&f.model_dir)?.recognize(&f.samples)?;
    assert!(!fresh.text.is_empty());
    assert_eq!(first, fresh);
    assert_eq!(second, fresh);
    Ok(())
}

#[test]
fn growing_spans_give_text_at_every_step() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let _one = one_at_a_time();
    let mut recognizer = OnnxRecognizer::open(&f.model_dir)?;
    let mut texts = Vec::new();
    for end in [SAMPLE_RATE_HZ * 3 / 2, SAMPLE_RATE_HZ * 3, f.samples.len()] {
        texts.push(recognizer.recognize(&f.samples[..end])?.text);
    }
    println!(
        "1.5 s: {}\n3.0 s: {}\nwhole: {}",
        texts[0], texts[1], texts[2]
    );
    assert!(texts[1].split_whitespace().count() >= 3, "{}", texts[1]);
    let correct = words_correct(&f.expected, &texts[2]);
    assert!(meets_pass_rule(correct), "{correct} words correct");
    Ok(())
}

#[test]
fn recognizer_registers_as_local_and_works_through_get() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let _one = one_at_a_time();
    let policy = PrivacyPolicy::local_only();
    let mut registry = EngineRegistry::new();
    registry.set_recognizer(Box::new(OnnxRecognizer::open(&f.model_dir)?), &policy)?;
    let engine: &mut dyn Recognizer = registry.get_recognizer(&policy)?;
    assert!(!engine.provider().is_remote());
    let recognition = engine.recognize(&f.samples)?;
    let correct = words_correct(&f.expected, &recognition.text);
    assert!(meets_pass_rule(correct), "{correct} words correct");
    Ok(())
}

#[test]
fn cached_decoding_gives_the_same_text_as_uncached_decoding() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let _one = one_at_a_time();
    let cached = OnnxRecognizer::open(&f.model_dir)?.recognize(&f.samples)?;
    let uncached = OnnxRecognizer::open(&f.model_dir)?
        .without_decoder_cache()
        .recognize(&f.samples)?;
    assert!(!cached.text.is_empty());
    assert_eq!(cached, uncached);
    Ok(())
}

#[test]
fn five_second_span_time_is_reported() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let _one = one_at_a_time();
    let mut span = f.samples.clone();
    span.resize(SPEED_SPAN_SAMPLES, 0.0);
    let loading = Instant::now();
    let mut recognizer = OnnxRecognizer::open(&f.model_dir)?;
    let load_ms = loading.elapsed().as_millis();
    let mut times = Vec::new();
    for _ in 0..SPEED_CALLS {
        let started = Instant::now();
        let recognition = recognizer.recognize(&span)?;
        times.push(started.elapsed().as_millis());
        assert!(!recognition.text.is_empty());
    }
    let first_ms = times[0];
    times.sort_unstable();
    println!(
        "load_ms={load_ms} first_ms={first_ms} median_ms={} max_ms={} span_samples={SPEED_SPAN_SAMPLES}",
        times[SPEED_CALLS / 2],
        times[SPEED_CALLS - 1]
    );
    Ok(())
}

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
