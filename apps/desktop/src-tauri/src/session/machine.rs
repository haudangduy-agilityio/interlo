//! What a session reads, loads and opens on this Mac. Tests pass their own.

use std::path::Path;

use audio_io::mic::SystemBackend;
use audio_io::{MicCapture, MicConfig};
use engine_host::traits::{Recognizer, Vad};
use engine_host::{EngineError, OnnxRecognizer, OnnxVad};

use crate::commands::setup::{SetupAnswer, SetupCheck};
use crate::session::capture::{self, Microphone, OpenFailed};

/// The engines that one load gives, before the privacy policy has seen them.
pub struct LoadedEngines {
    pub vad: Box<dyn Vad>,
    pub recognizer: Box<dyn Recognizer>,
}

/// The parts of the computer that a session uses.
pub trait Machine: Send + Sync {
    /// One setup check. `meeting_running` is asked when the microphone would be
    /// opened.
    fn setup(&self, meeting_running: &dyn Fn() -> bool) -> SetupAnswer;

    /// Load the speech engines from the model folder.
    fn engines(&self, folder: &Path) -> Result<LoadedEngines, EngineError>;

    /// Open the microphone of `config` and start to capture.
    fn open_microphone(&self, config: MicConfig) -> Result<Microphone, OpenFailed>;
}

/// This computer.
#[derive(Default)]
pub struct ThisMac {
    setup: SetupCheck,
}

impl Machine for ThisMac {
    fn setup(&self, meeting_running: &dyn Fn() -> bool) -> SetupAnswer {
        self.setup.answer_system(meeting_running)
    }

    fn engines(&self, folder: &Path) -> Result<LoadedEngines, EngineError> {
        let speech = folder.join("asr");
        Ok(LoadedEngines {
            vad: Box::new(OnnxVad::open(&speech.join("vad/vad.onnx"))?),
            recognizer: Box::new(OnnxRecognizer::open(&speech)?),
        })
    }

    fn open_microphone(&self, config: MicConfig) -> Result<Microphone, OpenFailed> {
        capture::open(MicCapture::new(config, Box::new(SystemBackend::new())))
    }
}
