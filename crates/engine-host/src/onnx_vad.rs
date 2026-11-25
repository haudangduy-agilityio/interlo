//! Voice activity detection with an ONNX model loaded in process.
//!
//! One call takes one frame of `FRAME_SAMPLES` samples (16 kHz mono) and returns
//! whether the frame is speech. The model keeps a recurrent state between frames
//! and needs the last 64 samples of the frame before; both live in the adapter,
//! so frames must arrive in order and `reset` starts a new stream.
//!
//! The adapter does not check the size or the SHA-256 of the model file. The
//! caller must pass a path that the model manager has verified in the same run,
//! before the call: a model that fails its checksum never runs (guardrail 7).
//! The check is owned by M1-04 and M1-42. The adapter opens no connection and
//! keeps no audio.
//!
//! The runtime writes its own log lines through `tracing`; those lines can hold
//! the model path and are not scrubbed. A log sink must filter the `ort` target.

use crate::error::EngineError;
use crate::speech_events::{check_frame, FRAME_SAMPLES};
use crate::traits::{Engine, Vad};
use meeting_core::policy::Provider;
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::OnceLock;

/// Samples of the previous frame that the model reads before the new frame.
const CONTEXT_SAMPLES: usize = 64;
/// Shape of the recurrent state.
const STATE_SHAPE: [usize; 3] = [2, 1, 128];
const STATE_VALUES: usize = STATE_SHAPE[0] * STATE_SHAPE[1] * STATE_SHAPE[2];
const SAMPLE_RATE_HZ: i64 = 16_000;
/// A frame with a probability at or above this is speech. Starting value:
/// M1-26 may tune it.
const DEFAULT_THRESHOLD: f32 = 0.5;

/// True when `probability` reaches `threshold`: a value equal to it is speech.
pub(crate) fn is_speech_probability(probability: f32, threshold: f32) -> bool {
    probability >= threshold
}

/// A threshold must lie above 0 and below 1. NaN is refused.
pub(crate) fn check_threshold(threshold: f32) -> Result<f32, EngineError> {
    if threshold > 0.0 && threshold < 1.0 {
        Ok(threshold)
    } else {
        Err(EngineError::Backend(
            "speech threshold must be above 0 and below 1".into(),
        ))
    }
}

/// The last component of a path: an error never carries the folder.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| "?".to_string(), |n| n.to_string_lossy().into_owned())
}

/// Runtime error text without any path. The model path as given becomes the
/// file name; any other word that holds a `/` (the same path in another form,
/// a file the model names) keeps only its last component.
pub(crate) fn scrub(text: &str, path: &Path) -> String {
    let text = text
        .replace(&path.to_string_lossy().into_owned(), &file_name(path))
        .replace(['\n', '\r'], " ");
    text.split(' ')
        .map(|word| word.rsplit('/').next().unwrap_or(word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn backend(what: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError::Backend(format!("voice activity model: {what}: {error}"))
}

/// The runtime allows one environment per process, and builds it once for the
/// life of the process. Every adapter shares this one.
pub(crate) fn shared_environment() -> Result<Environment, EngineError> {
    static ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();
    ENVIRONMENT
        .get_or_init(|| ort::init().build().map_err(|e| e.to_string()))
        .clone()
        .map_err(|e| backend("runtime start", e))
}

/// The voice activity engine. Local: it never leaves this Mac.
pub struct OnnxVad {
    session: Session,
    context: [f32; CONTEXT_SAMPLES],
    state: [f32; STATE_VALUES],
    threshold: f32,
    provider: Provider,
}

impl std::fmt::Debug for OnnxVad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxVad")
            .field("threshold", &self.threshold)
            .finish_non_exhaustive()
    }
}

impl OnnxVad {
    /// Load the model at `path` with one intra-op thread (the model is small).
    /// A missing or unreadable file is `EngineError::Backend` that names the
    /// file and not its folder. This call does not verify the file: the caller
    /// passes a path that the model manager has already checked (see the module
    /// doc). It reads the file, so it runs on a worker thread, never in an audio
    /// callback.
    pub fn open(path: &Path) -> Result<Self, EngineError> {
        let name = file_name(path);
        if !path.is_file() {
            return Err(EngineError::Backend(format!(
                "voice activity model file is missing: {name}"
            )));
        }
        let fail =
            |what: &str, e: &dyn std::fmt::Display| backend(what, scrub(&e.to_string(), path));
        let environment = shared_environment()?;
        let mut builder = Session::builder(&environment)
            .map_err(|e| fail("session builder", &e))?
            .with_intra_threads(1)
            .map_err(|e| fail("thread setting", &e))?;
        let session = builder
            .commit_from_file(path)
            .map_err(|e| fail(&format!("load {name}"), &e))?;
        Ok(Self {
            session,
            context: [0.0; CONTEXT_SAMPLES],
            state: [0.0; STATE_VALUES],
            threshold: DEFAULT_THRESHOLD,
            provider: Provider::local("onnx-vad"),
        })
    }

    /// Use a threshold other than 0.5. It must be above 0 and below 1.
    pub fn with_threshold(mut self, threshold: f32) -> Result<Self, EngineError> {
        self.threshold = check_threshold(threshold)?;
        Ok(self)
    }

    /// Start a new stream: forget the context and the recurrent state.
    pub fn reset(&mut self) {
        self.context = [0.0; CONTEXT_SAMPLES];
        self.state = [0.0; STATE_VALUES];
    }

    /// Speech probability of one frame. The context and the state advance only
    /// when the run succeeded.
    fn probability(&mut self, frame: &[f32]) -> Result<f32, EngineError> {
        check_frame(frame)?;
        if frame.iter().any(|sample| !sample.is_finite()) {
            return Err(EngineError::Backend(
                "a frame holds a sample that is not a finite number".into(),
            ));
        }
        let mut input = Vec::with_capacity(CONTEXT_SAMPLES + FRAME_SAMPLES);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(frame);
        let inputs = vec![
            (
                "input",
                Tensor::from_array(([1_usize, CONTEXT_SAMPLES + FRAME_SAMPLES], input))
                    .map_err(|e| backend("input tensor", e))?
                    .into_dyn(),
            ),
            (
                "state",
                Tensor::from_array((STATE_SHAPE, self.state.to_vec()))
                    .map_err(|e| backend("state tensor", e))?
                    .into_dyn(),
            ),
            (
                "sr",
                Tensor::from_array(([0_usize; 0], vec![SAMPLE_RATE_HZ]))
                    .map_err(|e| backend("rate tensor", e))?
                    .into_dyn(),
            ),
        ];
        let outputs = self.session.run(inputs).map_err(|e| backend("run", e))?;
        let probability = outputs
            .get("output")
            .ok_or_else(|| EngineError::Backend("voice activity model: no output".into()))?
            .try_extract_tensor::<f32>()
            .map_err(|e| backend("output", e))?
            .1
            .first()
            .copied()
            .ok_or_else(|| EngineError::Backend("voice activity model: empty output".into()))?;
        let next = outputs
            .get("stateN")
            .ok_or_else(|| EngineError::Backend("voice activity model: no state output".into()))?
            .try_extract_tensor::<f32>()
            .map_err(|e| backend("state output", e))?
            .1;
        if next.len() != STATE_VALUES {
            return Err(EngineError::Backend(format!(
                "voice activity model: state output has {} values",
                next.len()
            )));
        }
        self.state.copy_from_slice(next);
        self.context
            .copy_from_slice(&frame[FRAME_SAMPLES - CONTEXT_SAMPLES..]);
        Ok(probability)
    }
}

impl Engine for OnnxVad {
    fn provider(&self) -> &Provider {
        &self.provider
    }
}

impl Vad for OnnxVad {
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool, EngineError> {
        let probability = self.probability(frame)?;
        Ok(is_speech_probability(probability, self.threshold))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probability_equal_to_threshold_is_speech() {
        assert!(is_speech_probability(0.5, 0.5));
        assert!(!is_speech_probability(0.4999, 0.5));
        assert!(is_speech_probability(0.9, 0.5));
    }

    #[test]
    fn scrub_keeps_the_file_name_and_drops_every_folder() {
        let path = Path::new("/tmp/x/m.onnx");
        let absolute = scrub(
            "Load model from /tmp/x/m.onnx failed:Protobuf parsing failed.",
            path,
        );
        assert_eq!(
            absolute,
            "Load model from m.onnx failed:Protobuf parsing failed."
        );
        // The same file under a canonical folder name, and a file the model names.
        let other = scrub(
            "cannot open /private/tmp/x/m.onnx or /private/tmp/x/w.data",
            path,
        );
        assert!(!other.contains('/'), "{other}");
        assert!(
            other.contains("m.onnx") && other.contains("w.data"),
            "{other}"
        );
        // A relative path leaves the other words alone.
        let relative = scrub("failed parsing ./a/m.onnx", Path::new("./a/m.onnx"));
        assert_eq!(relative, "failed parsing m.onnx");
    }

    #[test]
    fn threshold_outside_open_range_is_refused() {
        for ok in [0.01_f32, 0.5, 0.99] {
            assert_eq!(check_threshold(ok), Ok(ok));
        }
        for bad in [0.0_f32, 1.0, -0.1, 1.5, f32::NAN] {
            assert!(
                matches!(check_threshold(bad), Err(EngineError::Backend(_))),
                "threshold {bad}"
            );
        }
    }
}
