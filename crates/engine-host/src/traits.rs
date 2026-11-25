//! Engine roles as traits, with plain data types at the boundary.
//! Samples are mono f32 at 16 kHz unless a method says otherwise.

use crate::error::EngineError;
use meeting_core::policy::Provider;
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

/// What every engine states about itself. The registry asks the privacy policy
/// about this provider before it takes the engine and before it hands it out.
/// Engines move to worker threads, so they are `Send`.
pub trait Engine: Send {
    /// Where the engine runs: `Provider::local` on this Mac, `Provider::remote`
    /// off it. No engine in v1 is remote (D-42).
    fn provider(&self) -> &Provider;
}

/// Voice activity detection on one frame.
pub trait Vad: Engine {
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool, EngineError>;
}

pub trait Recognizer: Engine {
    fn recognize(&mut self, samples: &[f32]) -> Result<Recognition, EngineError>;
}

pub trait Translator: Engine {
    fn translate(
        &mut self,
        text: &str,
        from: Language,
        to: Language,
    ) -> Result<String, EngineError>;
}

/// Text to speech. Returns mono f32 samples at the engine's output rate.
pub trait Synthesizer: Engine {
    fn synthesize(&mut self, text: &str, language: Language) -> Result<Vec<f32>, EngineError>;
}

pub trait SpeakerId: Engine {
    fn identify(&mut self, samples: &[f32]) -> Result<SpeakerLabel, EngineError>;
}

pub trait Summarizer: Engine {
    fn summarize(&mut self, transcript: &str, language: Language) -> Result<String, EngineError>;
}
