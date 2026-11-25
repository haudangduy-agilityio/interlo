//! Voice activity with the real model and a synthetic speech file.
//!
//! The model and the audio stay outside git. `INTERLO_MODELS_DIR` names the
//! folder that holds `M1-35/vad/vad.onnx` and `M1-36/vi_001.wav`. When the
//! variable is not set, each test prints one SKIP line and passes. When it is
//! set and a file is missing, the test fails and names the file.

use engine_host::{
    EngineError, EngineRegistry, OnnxVad, SpeechConfig, SpeechDetector, SpeechEvent, Vad,
    FRAME_SAMPLES,
};
use meeting_core::policy::PrivacyPolicy;
use std::path::{Path, PathBuf};
use std::time::Instant;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SKIP_LINE: &str = "SKIP real-model test: INTERLO_MODELS_DIR is not set";
const LEAD_IN_SAMPLES: usize = 8_000;
const TAIL_SAMPLES: usize = 16_000;
/// Half a second of speech at 16 kHz.
const MIN_SPEECH_SAMPLES: u64 = 8_000;
/// Frames compared between two adapters.
const COMPARE_FRAMES: usize = 40;

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

struct Fixture {
    model: PathBuf,
    padded: Vec<f32>,
}

/// The model path and the padded stream, or `None` when the test is skipped.
fn fixture() -> Result<Option<Fixture>, String> {
    let Some(dir) = models_dir() else {
        return Ok(None);
    };
    let model = require(&dir, "M1-35/vad/vad.onnx")?;
    let wav = read_wav(&require(&dir, "M1-36/vi_001.wav")?)?;
    let mut padded = vec![0.0_f32; LEAD_IN_SAMPLES];
    padded.extend_from_slice(&wav);
    padded.extend(std::iter::repeat_n(0.0_f32, TAIL_SAMPLES));
    Ok(Some(Fixture { model, padded }))
}

fn run_stream(
    vad: &mut dyn Vad,
    detector: &mut SpeechDetector,
    samples: &[f32],
) -> Result<Vec<SpeechEvent>, EngineError> {
    let mut events = Vec::new();
    for frame in samples.as_chunks::<FRAME_SAMPLES>().0 {
        if let Some(event) = detector.push(vad, frame)? {
            events.push(event);
        }
    }
    Ok(events)
}

fn check_speech_events(events: &[SpeechEvent], padded_len: usize) {
    assert_eq!(events.len(), 2, "{events:?}");
    let (SpeechEvent::Start { at_sample: start }, SpeechEvent::End { at_sample: end }) =
        (events[0], events[1])
    else {
        panic!("expected Start then End, got {events:?}");
    };
    assert!(
        start >= (LEAD_IN_SAMPLES - FRAME_SAMPLES) as u64,
        "start inside the lead-in: {start}"
    );
    assert!(
        end >= start + MIN_SPEECH_SAMPLES,
        "speech shorter than 0.5 s: {start}..{end}"
    );
    assert!(end <= padded_len as u64, "end after the stream: {end}");
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
fn zero_frame_is_not_speech_and_wrong_length_is_refused() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let mut vad = OnnxVad::open(&f.model)?;
    assert_eq!(vad.is_speech(&[0.0; FRAME_SAMPLES]), Ok(false));
    for len in [511_usize, 513] {
        assert!(matches!(
            vad.is_speech(&vec![0.0; len]),
            Err(EngineError::Backend(_))
        ));
    }
    Ok(())
}

#[test]
fn refused_frames_leave_the_adapter_state_unchanged() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let frames = f.padded[LEAD_IN_SAMPLES..].as_chunks::<FRAME_SAMPLES>().0;
    let (warm_up, rest) = frames.split_at(10);
    let mut touched = OnnxVad::open(&f.model)?;
    let mut fresh = OnnxVad::open(&f.model)?;
    for frame in warm_up {
        assert_eq!(touched.is_speech(frame), fresh.is_speech(frame));
    }
    // Speech samples, one too short or one too long, and one that is not finite.
    let mut not_finite = warm_up[0];
    not_finite[3] = f32::NAN;
    assert!(touched.is_speech(&rest[0][1..]).is_err());
    assert!(touched
        .is_speech(&[rest[0].as_slice(), &[0.5]].concat())
        .is_err());
    let text = touched
        .is_speech(&not_finite)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(!text.is_empty() && !text.contains("NaN"), "{text}");
    // The refused frames left no trace: both adapters answer alike afterwards.
    for frame in &rest[..COMPARE_FRAMES] {
        assert_eq!(touched.is_speech(frame), fresh.is_speech(frame));
    }
    Ok(())
}

#[test]
fn threshold_changes_which_frames_are_speech() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let count_speech = |threshold: f32| -> Result<usize, EngineError> {
        let mut vad = OnnxVad::open(&f.model)?.with_threshold(threshold)?;
        let mut count = 0;
        for frame in f.padded.as_chunks::<FRAME_SAMPLES>().0 {
            count += usize::from(vad.is_speech(frame)?);
        }
        Ok(count)
    };
    let (low, middle, high) = (count_speech(0.01)?, count_speech(0.5)?, count_speech(0.99)?);
    println!("speech frames: threshold 0.01 -> {low}, 0.5 -> {middle}, 0.99 -> {high}");
    assert!(low >= middle && middle >= high);
    assert!(low > high, "the threshold had no effect");
    assert!(matches!(
        OnnxVad::open(&f.model)?.with_threshold(1.0),
        Err(EngineError::Backend(_))
    ));
    Ok(())
}

#[test]
fn adapter_registers_as_local_and_is_gotten() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let policy = PrivacyPolicy::local_only();
    let mut registry = EngineRegistry::new();
    registry.set_vad(Box::new(OnnxVad::open(&f.model)?), &policy)?;
    let engine = registry.get_vad(&policy)?;
    assert!(!engine.provider().is_remote());
    Ok(())
}

#[test]
fn error_text_holds_no_sample_values() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let mut vad = OnnxVad::open(&f.model)?;
    let text = vad
        .is_speech(&[0.123_456_f32; 511])
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(!text.is_empty());
    assert!(!text.contains("0.123456"), "{text}");
    Ok(())
}

#[test]
fn speech_file_gives_one_start_and_one_end() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let mut vad = OnnxVad::open(&f.model)?;
    let mut detector = SpeechDetector::new(SpeechConfig::default());
    let events = run_stream(&mut vad, &mut detector, &f.padded)?;
    println!("events: {events:?} over {} samples", f.padded.len());
    check_speech_events(&events, f.padded.len());
    Ok(())
}

#[test]
fn second_run_after_reset_gives_the_same_events() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let mut vad = OnnxVad::open(&f.model)?;
    let mut detector = SpeechDetector::new(SpeechConfig::default());
    let first = run_stream(&mut vad, &mut detector, &f.padded)?;
    vad.reset();
    detector.reset();
    let second = run_stream(&mut vad, &mut detector, &f.padded)?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn detector_runs_through_get_vad() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let policy = PrivacyPolicy::local_only();
    let mut registry = EngineRegistry::new();
    registry.set_vad(Box::new(OnnxVad::open(&f.model)?), &policy)?;
    let mut detector = SpeechDetector::new(SpeechConfig::default());
    let events = run_stream(registry.get_vad(&policy)?, &mut detector, &f.padded)?;
    check_speech_events(&events, f.padded.len());
    let mut direct = OnnxVad::open(&f.model)?;
    let mut direct_detector = SpeechDetector::new(SpeechConfig::default());
    assert_eq!(
        events,
        run_stream(&mut direct, &mut direct_detector, &f.padded)?
    );
    Ok(())
}

#[test]
fn median_call_time_is_below_one_frame() -> TestResult {
    let Some(f) = fixture()? else { return Ok(()) };
    let loading = Instant::now();
    let mut vad = OnnxVad::open(&f.model)?;
    let load = loading.elapsed();
    let mut times = Vec::new();
    for frame in f.padded.as_chunks::<FRAME_SAMPLES>().0 {
        let started = Instant::now();
        vad.is_speech(frame)?;
        times.push(started.elapsed());
    }
    times.sort();
    let median = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    println!(
        "frames={} load={load:?} median={median:?} p95={p95:?}",
        times.len()
    );
    assert!(median.as_millis() < 32, "median {median:?}");
    Ok(())
}
