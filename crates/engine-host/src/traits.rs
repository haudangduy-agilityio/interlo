//! Engine roles as traits, with plain data types at the boundary.
//! Samples are mono f32 at 16 kHz unless a method says otherwise.

use crate::error::EngineError;
use meeting_core::protocol::Language;

/// Result of recognizing one speech span.
#[derive(Debug, Clone, PartialEq)]
pub struct Recognition {
    pub text: String,
    pub detected: Language,
}

/// Opaque label for a speaker, for example "speaker-1".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpeakerLabel(pub String);

/// Voice activity detection on one frame.
pub trait Vad {
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool, EngineError>;
}

pub trait Recognizer {
    fn recognize(&mut self, samples: &[f32]) -> Result<Recognition, EngineError>;
}

pub trait Translator {
    fn translate(
        &mut self,
        text: &str,
        from: Language,
        to: Language,
    ) -> Result<String, EngineError>;
}

/// Text to speech. Returns mono f32 samples at the engine's output rate.
pub trait Synthesizer {
    fn synthesize(&mut self, text: &str, language: Language) -> Result<Vec<f32>, EngineError>;
}

pub trait SpeakerId {
    fn identify(&mut self, samples: &[f32]) -> Result<SpeakerLabel, EngineError>;
}

pub trait Summarizer {
    fn summarize(&mut self, transcript: &str, language: Language) -> Result<String, EngineError>;
}
