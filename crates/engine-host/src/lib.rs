//! engine-host owns the voice-activity, recognition, translation, voice and
//! speaker models, plus the pure rules that sit around them (sentence
//! segmentation and the owner language gate).
//!
//! Must never open a network connection. Any network need goes through the
//! policy gate in `meeting-core`.
//! Must never hand out an engine that the privacy policy refuses: the registry
//! asks the policy at every `set_*` and at every `get_*`.

pub mod error;
pub mod language_gate;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
pub mod registry;
pub mod segmenter;
pub mod traits;

pub use error::{DenyCause, EngineError};
pub use language_gate::owner_should_translate;
pub use registry::{EngineRegistry, EngineRole};
pub use segmenter::{segment, Piece, Segmenter};
pub use traits::{
    Engine, Recognition, Recognizer, SpeakerId, SpeakerLabel, Summarizer, Synthesizer, Translator,
    Vad,
};
