//! Translation with the real model.
//!
//! The model stays outside git. `INTERLO_MODELS_DIR` names the folder that
//! holds `M1-35/translation/`. When the variable is not set, each real-model
//! test prints one SKIP line and passes. When it is set and a file is missing,
//! the test fails and names the file. The input sentences are rows of the
//! reading list, read at run time.

use engine_host::{EngineError, EngineRegistry, OnnxTranslator, Translator};
use meeting_core::policy::PrivacyPolicy;
use meeting_core::protocol::Language;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use tokenizers::normalizers::NFD;
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
const SPEED_CALLS: usize = 5;
/// The pairs of languages that the adapter refuses.
const REFUSED_PAIRS: [(Language, Language); 3] = [
    (Language::En, Language::Vi),
    (Language::Vi, Language::Vi),
    (Language::En, Language::En),
];
/// Words that the English text of row `vi_001` holds.
const EXPECTED_WORDS: [&str; 3] = ["payment", "button", "phone"];
/// Distinct words of one memory check: more tokens than the model has
/// positions.
const MEMORY_WORDS: u32 = 600;
/// Heap bytes that may stay after a call. Keeping the 600 words would take
/// many times more.
const KEPT_BYTES_LIMIT: isize = 4096;

thread_local! {
    static LIVE_BYTES: Cell<isize> = const { Cell::new(0) };
}

/// Counts the live heap bytes of each thread, so that one test can see what a
/// call leaves behind. Other threads do not change the number of this thread.
struct ThreadBytes;

fn count(change: isize) {
    // A thread that is shutting down has no counter left, and nothing reads it.
    let _ = LIVE_BYTES.try_with(|live| live.set(live.get() + change));
}

// A global allocator is the only way to see what stays on the heap. Every call
// goes to the system allocator unchanged. The test
// `translator_keeps_no_input_word_after_the_call` covers it.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for ThreadBytes {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size() as isize);
        // The caller gives the same guarantees as for the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(-(layout.size() as isize));
        // The pointer and layout come from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size as isize - layout.size() as isize);
        // The pointer and layout come from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: ThreadBytes = ThreadBytes;

/// Real-model tests run one at a time: each loads a large model, and a
/// parallel test would distort the speed report.
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

/// The translation model folder, or `None` after printing the skip line.
fn model_dir() -> Result<Option<PathBuf>, String> {
    let value = std::env::var("INTERLO_MODELS_DIR").ok();
    let dir = match resolve(value.as_deref()) {
        Ok(dir) => dir,
        Err(line) => {
            println!("{line}");
            return Ok(None);
        }
    };
    for name in MODEL_FILES {
        require(&dir, &format!("M1-35/translation/{name}"))?;
    }
    Ok(Some(dir.join("M1-35/translation")))
}

/// The sentence of one row of the reading list, read at run time.
fn row(id: &str) -> Result<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/m0/reading-list.md");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read reading list: {e}"))?;
    for line in text.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() >= 4 && cells[1] == id {
            return Ok(cells[3].to_string());
        }
    }
    Err(format!("row {id} not found in the reading list"))
}

fn to_english(translator: &mut dyn Translator, text: &str) -> Result<String, EngineError> {
    translator.translate(text, Language::Vi, Language::En)
}

fn backend_text(result: Result<String, EngineError>) -> String {
    match result {
        Err(EngineError::Backend(text)) => text,
        other => format!("not a Backend error: {other:?}"),
    }
}

/// The pass rule for the English text of row `vi_001`: it holds the expected
/// words, and no Vietnamese letter is left.
fn assert_english(output: &str) {
    let lower = output.to_lowercase();
    for word in EXPECTED_WORDS {
        assert!(lower.contains(word), "no {word} in: {output}");
    }
    assert!(
        output.chars().all(|c| !c.is_alphabetic() || c.is_ascii()),
        "a letter outside ASCII in: {output}"
    );
    assert!(
        !output.chars().any(|c| ('\u{300}'..='\u{36f}').contains(&c)),
        "a combining mark in: {output}"
    );
}

#[test]
fn real_model_tests_skip_with_a_message_when_the_folder_is_not_set() {
    assert_eq!(resolve(None), Err(SKIP_LINE));
    assert_eq!(resolve(Some("/models")), Ok(PathBuf::from("/models")));
}

#[test]
fn real_model_test_fails_when_a_file_is_missing() {
    let error = require(
        Path::new("/nonexistent-models-folder"),
        "M1-35/translation/config.json",
    );
    assert_eq!(
        error,
        Err("missing file for the real-model test: M1-35/translation/config.json".to_string())
    );
}

#[test]
fn test_sentence_is_translated_to_english() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    let output = to_english(&mut translator, &row("vi_001")?)?;
    println!("output: {output}");
    assert_english(&output);
    Ok(())
}

#[test]
fn lower_case_input_gives_the_output_of_the_original_case() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    let sentence = row("vi_001")?;
    let lower = sentence.to_lowercase();
    assert_ne!(lower, sentence);
    let original = to_english(&mut translator, &sentence)?;
    assert!(!original.is_empty());
    assert_eq!(to_english(&mut translator, &lower)?, original);
    Ok(())
}

#[test]
fn decomposed_input_gives_the_output_of_the_composed_text() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    let sentence = row("vi_001")?;
    let mut decomposed = NormalizedString::from(sentence.as_str());
    NFD.normalize(&mut decomposed).map_err(|e| e.to_string())?;
    assert_ne!(decomposed.get(), sentence);
    let composed = to_english(&mut translator, &sentence)?;
    assert!(!composed.is_empty());
    assert_eq!(to_english(&mut translator, decomposed.get())?, composed);
    Ok(())
}

#[test]
fn text_without_a_letter_or_digit_returns_an_empty_output() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    for empty in ["", "   ", ".", "?! \u{2026}"] {
        assert_eq!(
            to_english(&mut translator, empty),
            Ok(String::new()),
            "{empty:?}"
        );
    }
    Ok(())
}

#[test]
fn other_language_pairs_are_refused_by_the_adapter() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    let sentence = row("vi_001")?;
    for (from, to) in REFUSED_PAIRS {
        let error = backend_text(translator.translate(&sentence, from, to));
        assert!(error.contains("only Vietnamese to English"), "{error}");
    }
    // The pair is checked before the text.
    let error = backend_text(translator.translate("", Language::En, Language::Vi));
    assert!(error.contains("only Vietnamese to English"), "{error}");
    Ok(())
}

#[test]
fn long_input_is_refused_with_a_typed_error() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    let long = vec![row("vi_001")?; 40].join(" ");
    assert_eq!(
        backend_text(to_english(&mut translator, &long)),
        "translation model: the input is too long"
    );
    Ok(())
}

#[test]
fn repeating_output_of_the_model_is_a_typed_error() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    let result = to_english(&mut translator, &row("vi_021")?);
    // The text of a wrong result is not printed.
    assert!(result.is_err(), "row vi_021 gave a text");
    assert_eq!(
        backend_text(result),
        "translation model: the output repeats"
    );
    Ok(())
}

#[test]
fn cached_translation_gives_the_same_output_as_uncached() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut cached = OnnxTranslator::open(&dir)?;
    let mut uncached = OnnxTranslator::open(&dir)?.without_decoder_cache();
    // The two adapters must differ, else the comparison shows nothing.
    assert!(format!("{cached:?}").contains("use_cache: true"));
    assert!(format!("{uncached:?}").contains("use_cache: false"));
    let one = row("vi_001")?;
    let two = format!("{one} {}", row("vi_002")?);
    for input in [one, two] {
        let output = to_english(&mut cached, &input)?;
        println!("output: {output}");
        assert!(!output.is_empty());
        assert_eq!(to_english(&mut uncached, &input)?, output);
    }
    Ok(())
}

#[test]
fn calls_after_refused_inputs_give_the_output_of_a_fresh_adapter() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut used = OnnxTranslator::open(&dir)?;
    let sentence = row("vi_001")?;
    for (from, to) in REFUSED_PAIRS {
        assert!(used.translate(&sentence, from, to).is_err());
    }
    assert!(used.translate("", Language::En, Language::Vi).is_err());
    let long = vec![sentence.as_str(); 40].join(" ");
    assert!(to_english(&mut used, &long).is_err());
    assert!(!to_english(&mut used, &row("vi_002")?)?.is_empty());
    let first = to_english(&mut used, &sentence)?;
    let second = to_english(&mut used, &sentence)?;
    let fresh = to_english(&mut OnnxTranslator::open(&dir)?, &sentence)?;
    assert!(!fresh.is_empty());
    assert_eq!(first, fresh);
    assert_eq!(second, fresh);
    Ok(())
}

/// `MEMORY_WORDS` numbers from `first`, each one a different word.
fn distinct_words(first: u32) -> String {
    let words: Vec<String> = (first..first + MEMORY_WORDS)
        .map(|n| n.to_string())
        .collect();
    words.join(" ")
}

#[test]
fn translator_keeps_no_input_word_after_the_call() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let mut translator = OnnxTranslator::open(&dir)?;
    let warm_up = distinct_words(100_000);
    let measured = distinct_words(200_000);
    // The first call may set up state that stays; the second one is measured.
    assert!(to_english(&mut translator, &warm_up).is_err());
    let before = LIVE_BYTES.get();
    // Refused for its length after the words were split into tokens.
    let error = backend_text(to_english(&mut translator, &measured));
    assert_eq!(error, "translation model: the input is too long");
    drop(error);
    let kept = LIVE_BYTES.get() - before;
    assert!(kept < KEPT_BYTES_LIMIT, "{kept} bytes stay after the call");
    Ok(())
}

#[test]
fn translator_registers_as_local_and_works_through_get() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let policy = PrivacyPolicy::local_only();
    let mut registry = EngineRegistry::new();
    registry.set_translator(Box::new(OnnxTranslator::open(&dir)?), &policy)?;
    let engine: &mut dyn Translator = registry.get_translator(&policy)?;
    assert!(!engine.provider().is_remote());
    let output = to_english(engine, &row("vi_001")?)?;
    assert_english(&output);
    Ok(())
}

#[test]
fn translation_time_of_the_test_sentence_is_reported() -> TestResult {
    let Some(dir) = model_dir()? else {
        return Ok(());
    };
    let _one = one_at_a_time();
    let sentence = row("vi_001")?;
    let loading = Instant::now();
    let mut translator = OnnxTranslator::open(&dir)?;
    let load_ms = loading.elapsed().as_millis();
    let mut times = Vec::new();
    for _ in 0..SPEED_CALLS {
        let started = Instant::now();
        let output = to_english(&mut translator, &sentence)?;
        times.push(started.elapsed().as_millis());
        assert!(!output.is_empty());
    }
    let first_ms = times[0];
    times.sort_unstable();
    println!(
        "load_ms={load_ms} first_ms={first_ms} median_ms={} max_ms={} input_row=vi_001",
        times[SPEED_CALLS / 2],
        times[SPEED_CALLS - 1]
    );
    Ok(())
}
