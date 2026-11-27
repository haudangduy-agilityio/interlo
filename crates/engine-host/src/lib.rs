//! engine-host owns the voice-activity, recognition, translation, voice and
//! speaker models, plus the pure rules that sit around them (sentence
//! segmentation, the owner language gate and the span rules), and the speech
//! chain from captured samples to captions.
//!
//! Must never open a network connection. Any network need goes through the
//! policy gate in `meeting-core`.
//! Must never hand out an engine that the privacy policy refuses: the registry
//! asks the policy at every `set_*` and at every `get_*`.
//! Must never send anything of a session after its stop.

pub mod error;
pub mod language_gate;
pub mod live_chain;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
pub mod onnx_asr;
mod onnx_seq2seq;
pub mod onnx_translate;
pub mod onnx_vad;
pub mod registry;
pub mod segmenter;
pub mod speech_events;
pub mod speech_features;
pub mod traits;
mod translation_text;
pub mod utterance;

pub use error::{DenyCause, EngineError};
pub use language_gate::owner_should_translate;
pub use onnx_asr::OnnxRecognizer;
pub use onnx_translate::OnnxTranslator;
pub use onnx_vad::OnnxVad;
pub use registry::{EngineRegistry, EngineRole};
pub use segmenter::{segment, Piece, Segmenter};
pub use speech_events::{SpeechConfig, SpeechDetector, SpeechEvent, FRAME_SAMPLES};
pub use traits::{
    Engine, Recognition, Recognizer, SpeakerId, SpeakerLabel, Summarizer, Synthesizer, Translator,
    Vad,
};
