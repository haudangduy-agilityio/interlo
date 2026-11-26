//! The whole speech chain with the real models and a synthetic speech file.
//!
//! The models and the audio stay outside git. `INTERLO_MODELS_DIR` names the
//! folder that holds `M1-35/vad/vad.onnx`, `M1-35/asr/` and
//! `M1-36/vi_001.wav`. When the variable is not set, each test prints one SKIP
//! line and passes. When it is set and a file is missing, the test fails and
//! names the file.

// The sink of this file and one test use `unwrap`.
#![allow(clippy::unwrap_used)]

use engine_host::live_chain::{
    Caption, ChainEnd, ChainSink, Engines, FrameSource, Latency, LiveChain, MonotonicClock,
};
use engine_host::{OnnxRecognizer, OnnxVad, Vad, FRAME_SAMPLES};
use meeting_core::policy::PrivacyPolicy;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokenizers::normalizers::NFC;
use tokenizers::{NormalizedString, Normalizer};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SKIP_LINE: &str = "SKIP real-model test: INTERLO_MODELS_DIR is not set";
const LEAD_IN_SAMPLES: usize = 8_000;
const TAIL_SAMPLES: usize = 16_000;
/// Words that must match for a pass: 13 of the 15 words of the sentence.
const PASS_WORDS: usize = 13;
/// The longest the chain may take for the sentence in a debug build.
const WAIT: Duration = Duration::from_secs(180);

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
/// once.
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

/// Hands out the stream, then reports through `drained` how often the reader
/// came back after the last sample.
struct Stream {
    samples: Vec<f32>,
    at: usize,
    drained: Arc<AtomicU32>,
}

impl FrameSource for Stream {
    fn read(&mut self, out: &mut [f32]) -> usize {
        let count = out.len().min(self.samples.len() - self.at);
        out[..count].copy_from_slice(&self.samples[self.at..self.at + count]);
        self.at += count;
        if count == 0 {
            self.drained.fetch_add(1, Ordering::SeqCst);
        }
        count
    }

    fn failed(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct Heard {
    captions: Vec<Caption>,
    failures: usize,
    ends: Vec<ChainEnd>,
}

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Heard>>);

impl ChainSink for Sink {
    fn caption(&mut self, caption: Caption) {
        self.0.lock().unwrap().captions.push(caption);
    }

    fn latency(&mut self, _latency: Latency) {}

    fn recognition_failed(&mut self) {
        self.0.lock().unwrap().failures += 1;
    }

    fn ended(&mut self, end: ChainEnd) {
        self.0.lock().unwrap().ends.push(end);
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
fn real_chain_captions_the_test_sentence() -> TestResult {
    let Some(dir) = models_dir() else {
        return Ok(());
    };
    let detector = require(&dir, "M1-35/vad/vad.onnx")?;
    for name in [
        "encoder.onnx",
        "decoder.onnx",
        "tokenizer.json",
        "config.json",
    ] {
        require(&dir, &format!("M1-35/asr/{name}"))?;
    }
    let speech = read_wav(&require(&dir, "M1-36/vi_001.wav")?)?;
    let mut samples = vec![0.0_f32; LEAD_IN_SAMPLES];
    samples.extend_from_slice(&speech);
    samples.extend(std::iter::repeat_n(0.0_f32, TAIL_SAMPLES));

    let engines = Engines::new(
        Box::new(OnnxVad::open(&detector)?),
        Box::new(OnnxRecognizer::open(&dir.join("M1-35/asr"))?),
        Arc::new(RwLock::new(PrivacyPolicy::local_only())),
    )?;
    let drained = Arc::new(AtomicU32::new(0));
    let sink = Sink::default();
    let chain = LiveChain::start(
        Box::new(Stream {
            samples,
            at: 0,
            drained: Arc::clone(&drained),
        }),
        Arc::new(engines),
        Arc::new(MonotonicClock::default()),
        Box::new(sink.clone()),
    )?;
    let started = Instant::now();
    while drained.load(Ordering::SeqCst) < 2 || chain.pending_spans() > 0 {
        assert!(started.elapsed() < WAIT, "the chain did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }
    chain.stop();

    let heard = sink.0.lock().unwrap();
    assert_eq!((heard.failures, heard.ends.as_slice()), (0, &[][..]));
    for caption in &heard.captions {
        println!("caption: {} ({} ms)", caption.text, caption.delay_ms);
    }
    let text = heard
        .captions
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let expected = sentence()?;
    let correct = words_correct(&expected, &text);
    println!("words correct: {correct} of {}", words(&expected).len());
    assert_eq!(words(&expected).len(), 15);
    assert!(correct >= PASS_WORDS, "{correct} words correct");
    Ok(())
}

/// Frames of the padded stream that end inside the speech.
const MID_SPEECH_FRAMES: usize = 80;

fn answers(vad: &mut dyn Vad, samples: &[f32]) -> Result<Vec<bool>, engine_host::EngineError> {
    samples
        .as_chunks::<FRAME_SAMPLES>()
        .0
        .iter()
        .map(|frame| vad.is_speech(frame))
        .collect()
}

/// The chain resets the detector through the `Vad` trait, at the start of each
/// session.
#[test]
fn reset_through_the_trait_gives_the_answers_of_a_fresh_detector() -> TestResult {
    let Some(dir) = models_dir() else {
        return Ok(());
    };
    let model = require(&dir, "M1-35/vad/vad.onnx")?;
    let speech = read_wav(&require(&dir, "M1-36/vi_001.wav")?)?;
    let mut samples = vec![0.0_f32; LEAD_IN_SAMPLES];
    samples.extend_from_slice(&speech);
    let head = &samples[..MID_SPEECH_FRAMES * FRAME_SAMPLES];
    let tail = &samples[LEAD_IN_SAMPLES..];

    let mut leftovers = 0;
    for threshold in [0.01_f32, 0.5, 0.99] {
        let open = || OnnxVad::open(&model).and_then(|vad| vad.with_threshold(threshold));
        let fresh = answers(&mut open()?, tail)?;
        let mut used = open()?;
        answers(&mut used, head)?;
        leftovers += usize::from(answers(&mut used, tail)? != fresh);

        let mut reset = open()?;
        answers(&mut reset, head)?;
        let engine: &mut dyn Vad = &mut reset;
        engine.reset();
        assert_eq!(answers(engine, tail)?, fresh, "threshold {threshold}");
    }
    // Without a reset the answers differ somewhere, so the check above can fail.
    assert!(leftovers > 0, "the state of the detector had no effect");
    Ok(())
}
