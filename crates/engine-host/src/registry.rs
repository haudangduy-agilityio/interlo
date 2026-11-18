//! Holds one boxed engine per role and reports which roles are not loaded.

use crate::traits::{Recognizer, SpeakerId, Summarizer, Synthesizer, Translator, Vad};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineRole {
    Vad,
    Recognizer,
    Translator,
    Synthesizer,
    SpeakerId,
    Summarizer,
}

#[derive(Default)]
pub struct EngineRegistry {
    vad: Option<Box<dyn Vad + Send>>,
    recognizer: Option<Box<dyn Recognizer + Send>>,
    translator: Option<Box<dyn Translator + Send>>,
    synthesizer: Option<Box<dyn Synthesizer + Send>>,
    speaker_id: Option<Box<dyn SpeakerId + Send>>,
    summarizer: Option<Box<dyn Summarizer + Send>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_vad(&mut self, e: Box<dyn Vad + Send>) {
        self.vad = Some(e);
    }

    pub fn set_recognizer(&mut self, e: Box<dyn Recognizer + Send>) {
        self.recognizer = Some(e);
    }

    pub fn set_translator(&mut self, e: Box<dyn Translator + Send>) {
        self.translator = Some(e);
    }

    pub fn set_synthesizer(&mut self, e: Box<dyn Synthesizer + Send>) {
        self.synthesizer = Some(e);
    }

    pub fn set_speaker_id(&mut self, e: Box<dyn SpeakerId + Send>) {
        self.speaker_id = Some(e);
    }

    pub fn set_summarizer(&mut self, e: Box<dyn Summarizer + Send>) {
        self.summarizer = Some(e);
    }

    /// Roles with no engine loaded, in a fixed order.
    pub fn missing_roles(&self) -> Vec<EngineRole> {
        let checks = [
            (EngineRole::Vad, self.vad.is_some()),
            (EngineRole::Recognizer, self.recognizer.is_some()),
            (EngineRole::Translator, self.translator.is_some()),
            (EngineRole::Synthesizer, self.synthesizer.is_some()),
            (EngineRole::SpeakerId, self.speaker_id.is_some()),
            (EngineRole::Summarizer, self.summarizer.is_some()),
        ];
        checks
            .into_iter()
            .filter(|(_, loaded)| !loaded)
            .map(|(role, _)| role)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EngineError;
    use meeting_core::protocol::Language;

    struct FakeVad;
    impl Vad for FakeVad {
        fn is_speech(&mut self, _frame: &[f32]) -> Result<bool, EngineError> {
            Ok(true)
        }
    }

    struct FakeTranslator;
    impl Translator for FakeTranslator {
        fn translate(
            &mut self,
            text: &str,
            _: Language,
            _: Language,
        ) -> Result<String, EngineError> {
            Ok(text.to_string())
        }
    }

    #[test]
    fn empty_registry_misses_every_role() {
        let reg = EngineRegistry::new();
        assert_eq!(
            reg.missing_roles(),
            vec![
                EngineRole::Vad,
                EngineRole::Recognizer,
                EngineRole::Translator,
                EngineRole::Synthesizer,
                EngineRole::SpeakerId,
                EngineRole::Summarizer,
            ]
        );
    }

    #[test]
    fn loading_an_engine_removes_its_role_from_missing() {
        let mut reg = EngineRegistry::new();
        reg.set_vad(Box::new(FakeVad));
        reg.set_translator(Box::new(FakeTranslator));
        let missing = reg.missing_roles();
        assert_eq!(missing.len(), 4);
        assert!(!missing.contains(&EngineRole::Vad));
        assert!(!missing.contains(&EngineRole::Translator));
        assert!(missing.contains(&EngineRole::Recognizer));
    }

    #[test]
    fn registry_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<EngineRegistry>();
    }
}
