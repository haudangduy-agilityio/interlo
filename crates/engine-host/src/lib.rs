//! engine-host owns the voice-activity, recognition, translation, voice and
//! speaker models, plus the pure rules that sit around them (sentence
//! segmentation and the owner language gate).
//!
//! Must never open a network connection. Any network need goes through the
//! policy gate in `meeting-core`.

pub mod error;
pub mod language_gate;
pub mod registry;
pub mod segmenter;
pub mod traits;

pub use error::EngineError;
pub use language_gate::owner_should_translate;
pub use registry::{EngineRegistry, EngineRole};
pub use segmenter::{segment, Piece, Segmenter};
pub use traits::{
    Recognition, Recognizer, SpeakerId, SpeakerLabel, Summarizer, Synthesizer, Translator, Vad,
};
