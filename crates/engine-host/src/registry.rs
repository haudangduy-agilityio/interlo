//! Holds one boxed engine per role. The privacy policy decides whether an
//! engine may enter and whether it may be used: every `set_*` and every `get_*`
//! asks it first. A role with no engine is reported, never guessed.

use crate::error::{DenyCause, EngineError};
use crate::traits::{Recognizer, SpeakerId, Summarizer, Synthesizer, Translator, Vad};
use meeting_core::policy::{EgressDenied, PrivacyPolicy, Provider};
use meeting_core::protocol::DataKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineRole {
    Vad,
    Recognizer,
    Translator,
    Synthesizer,
    SpeakerId,
    Summarizer,
}

impl EngineRole {
    /// The kind of data this role sends to its provider: audio for the roles
    /// that listen, text for the roles that read text.
    pub fn data_kind(self) -> DataKind {
        match self {
            Self::Vad | Self::Recognizer | Self::SpeakerId => DataKind::Audio,
            Self::Translator | Self::Synthesizer | Self::Summarizer => DataKind::Text,
        }
    }
}

/// Ask the policy whether `provider` may receive the data of `role`.
fn admit(role: EngineRole, provider: &Provider, policy: &PrivacyPolicy) -> Result<(), EngineError> {
    policy
        .check(provider, role.data_kind())
        .map_err(|denied| denial(role, provider, denied))
}

fn denial(role: EngineRole, provider: &Provider, denied: EgressDenied) -> EngineError {
    let cause = match denied {
        EgressDenied::LocalOnly(_) => DenyCause::LocalOnly,
        EgressDenied::NoConsent(_, kind) => DenyCause::NoConsent(kind),
        // `PrivacyPolicy::check` never returns these; they belong to the
        // download paths and to the egress guard, which have no engine role.
        // Defensive fallback: a refusal never turns into an accepted engine.
        EgressDenied::ModelHostNotAllowed(_)
        | EgressDenied::DriverHostNotAllowed(_)
        | EgressDenied::MeetingRunning => {
            return EngineError::Backend("this refusal is not an engine data path".into())
        }
    };
    EngineError::Denied {
        role,
        provider: provider.id().to_string(),
        cause,
    }
}

#[derive(Default)]
pub struct EngineRegistry {
    vad: Option<Box<dyn Vad>>,
    recognizer: Option<Box<dyn Recognizer>>,
    translator: Option<Box<dyn Translator>>,
    synthesizer: Option<Box<dyn Synthesizer>>,
    speaker_id: Option<Box<dyn SpeakerId>>,
    summarizer: Option<Box<dyn Summarizer>>,
}

/// One `set_*` and one `get_*` per role. `set_*` writes the slot only after the
/// policy accepted the engine, so a refusal keeps the engine already there.
macro_rules! role_methods {
    ($role:ident, $field:ident, $set:ident, $get:ident) => {
        pub fn $set(
            &mut self,
            engine: Box<dyn $role>,
            policy: &PrivacyPolicy,
        ) -> Result<(), EngineError> {
            admit(EngineRole::$role, engine.provider(), policy)?;
            self.$field = Some(engine);
            Ok(())
        }

        pub fn $get(&mut self, policy: &PrivacyPolicy) -> Result<&mut dyn $role, EngineError> {
            let engine = self
                .$field
                .as_deref_mut()
                .ok_or(EngineError::NotLoaded(EngineRole::$role))?;
            admit(EngineRole::$role, engine.provider(), policy)?;
            Ok(engine)
        }
    };
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    role_methods!(Vad, vad, set_vad, get_vad);
    role_methods!(Recognizer, recognizer, set_recognizer, get_recognizer);
    role_methods!(Translator, translator, set_translator, get_translator);
    role_methods!(Synthesizer, synthesizer, set_synthesizer, get_synthesizer);
    role_methods!(SpeakerId, speaker_id, set_speaker_id, get_speaker_id);
    role_methods!(Summarizer, summarizer, set_summarizer, get_summarizer);

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
    use crate::mock::{Calls, Mock};
    use crate::traits::{Recognition, SpeakerLabel};
    use meeting_core::protocol::Language;

    const ALL_ROLES: [EngineRole; 6] = [
        EngineRole::Vad,
        EngineRole::Recognizer,
        EngineRole::Translator,
        EngineRole::Synthesizer,
        EngineRole::SpeakerId,
        EngineRole::Summarizer,
    ];

    fn local_only_with_consent(provider: &str) -> PrivacyPolicy {
        let mut policy = PrivacyPolicy::default();
        policy.grant(provider, DataKind::Audio);
        policy.grant(provider, DataKind::Text);
        policy
    }

    fn open_policy() -> PrivacyPolicy {
        let mut policy = PrivacyPolicy::default();
        policy.set_local_only(false);
        policy
    }

    fn mock<V>(value: V, remote: Option<&str>) -> Mock<V> {
        match remote {
            Some(id) => Mock::new(value).remote(id),
            None => Mock::new(value),
        }
    }

    /// Register a mock for `role`: local, or remote when `remote` names the
    /// provider. Returns the registration result and the mock's call counter.
    fn register(
        reg: &mut EngineRegistry,
        role: EngineRole,
        remote: Option<&str>,
        policy: &PrivacyPolicy,
    ) -> (Result<(), EngineError>, Calls) {
        match role {
            EngineRole::Vad => {
                let m = mock(true, remote);
                let calls = m.calls();
                (reg.set_vad(Box::new(m), policy), calls)
            }
            EngineRole::Recognizer => {
                let m = mock(
                    Recognition {
                        text: "xin chao".into(),
                        detected: Language::Vi,
                    },
                    remote,
                );
                let calls = m.calls();
                (reg.set_recognizer(Box::new(m), policy), calls)
            }
            EngineRole::Translator => {
                let m = mock(String::from("hello"), remote);
                let calls = m.calls();
                (reg.set_translator(Box::new(m), policy), calls)
            }
            EngineRole::Synthesizer => {
                let m = mock(vec![0.5_f32], remote);
                let calls = m.calls();
                (reg.set_synthesizer(Box::new(m), policy), calls)
            }
            EngineRole::SpeakerId => {
                let m = mock(SpeakerLabel("speaker-1".into()), remote);
                let calls = m.calls();
                (reg.set_speaker_id(Box::new(m), policy), calls)
            }
            EngineRole::Summarizer => {
                let m = mock(String::from("notes"), remote);
                let calls = m.calls();
                (reg.set_summarizer(Box::new(m), policy), calls)
            }
        }
    }

    /// Get the engine of `role` from the registry and call it once.
    fn get_and_call(
        reg: &mut EngineRegistry,
        role: EngineRole,
        policy: &PrivacyPolicy,
    ) -> Result<(), EngineError> {
        match role {
            EngineRole::Vad => reg.get_vad(policy)?.is_speech(&[0.0]).map(|_| ()),
            EngineRole::Recognizer => reg.get_recognizer(policy)?.recognize(&[0.0]).map(|_| ()),
            EngineRole::Translator => reg
                .get_translator(policy)?
                .translate("a", Language::Vi, Language::En)
                .map(|_| ()),
            EngineRole::Synthesizer => reg
                .get_synthesizer(policy)?
                .synthesize("a", Language::En)
                .map(|_| ()),
            EngineRole::SpeakerId => reg.get_speaker_id(policy)?.identify(&[0.0]).map(|_| ()),
            EngineRole::Summarizer => reg
                .get_summarizer(policy)?
                .summarize("a", Language::En)
                .map(|_| ()),
        }
    }

    fn denied(role: EngineRole, provider: &str, cause: DenyCause) -> EngineError {
        EngineError::Denied {
            role,
            provider: provider.to_string(),
            cause,
        }
    }

    #[test]
    fn new_registry_misses_all_six_roles_in_order() {
        assert_eq!(EngineRegistry::new().missing_roles(), ALL_ROLES.to_vec());
    }

    #[test]
    fn registered_role_leaves_the_missing_list() {
        let policy = PrivacyPolicy::default();
        for role in ALL_ROLES {
            let mut reg = EngineRegistry::new();
            assert_eq!(register(&mut reg, role, None, &policy).0, Ok(()));
            let expected: Vec<EngineRole> = ALL_ROLES.into_iter().filter(|r| *r != role).collect();
            assert_eq!(reg.missing_roles(), expected, "{role:?}");
        }
    }

    #[test]
    fn mock_of_every_role_answers_through_get() {
        let policy = PrivacyPolicy::default();
        let mut reg = EngineRegistry::new();
        for role in ALL_ROLES {
            assert_eq!(register(&mut reg, role, None, &policy).0, Ok(()));
        }
        assert!(reg.missing_roles().is_empty());
        assert_eq!(
            reg.get_vad(&policy).map(|e| e.is_speech(&[0.0])),
            Ok(Ok(true))
        );
        assert_eq!(
            reg.get_recognizer(&policy).map(|e| e.recognize(&[0.0])),
            Ok(Ok(Recognition {
                text: "xin chao".into(),
                detected: Language::Vi
            }))
        );
        assert_eq!(
            reg.get_translator(&policy)
                .map(|e| e.translate("a", Language::Vi, Language::En)),
            Ok(Ok("hello".to_string()))
        );
        assert_eq!(
            reg.get_synthesizer(&policy)
                .map(|e| e.synthesize("a", Language::En)),
            Ok(Ok(vec![0.5_f32]))
        );
        assert_eq!(
            reg.get_speaker_id(&policy).map(|e| e.identify(&[0.0])),
            Ok(Ok(SpeakerLabel("speaker-1".into())))
        );
        assert_eq!(
            reg.get_summarizer(&policy)
                .map(|e| e.summarize("a", Language::En)),
            Ok(Ok("notes".to_string()))
        );
    }

    #[test]
    fn local_engine_registers_and_gets_for_every_role() {
        let policy = PrivacyPolicy::default();
        for role in ALL_ROLES {
            let mut reg = EngineRegistry::new();
            let (registered, calls) = register(&mut reg, role, None, &policy);
            assert_eq!(registered, Ok(()), "{role:?}");
            assert_eq!(get_and_call(&mut reg, role, &policy), Ok(()), "{role:?}");
            assert_eq!(calls.count(), 1, "{role:?}");
        }
    }

    #[test]
    fn remote_engine_is_refused_in_local_only_mode_for_every_role() {
        // A consent exists for both kinds, and local-only still wins.
        let policy = local_only_with_consent("remote-p");
        for role in ALL_ROLES {
            let mut reg = EngineRegistry::new();
            let (registered, calls) = register(&mut reg, role, Some("remote-p"), &policy);
            assert_eq!(
                registered,
                Err(denied(role, "remote-p", DenyCause::LocalOnly)),
                "{role:?}"
            );
            assert_eq!(reg.missing_roles(), ALL_ROLES.to_vec(), "{role:?}");
            assert_eq!(calls.count(), 0, "{role:?}");
        }
    }

    #[test]
    fn role_data_kind_follows_what_the_role_receives() {
        for role in [
            EngineRole::Vad,
            EngineRole::Recognizer,
            EngineRole::SpeakerId,
        ] {
            assert_eq!(role.data_kind(), DataKind::Audio, "{role:?}");
        }
        for role in [
            EngineRole::Translator,
            EngineRole::Synthesizer,
            EngineRole::Summarizer,
        ] {
            assert_eq!(role.data_kind(), DataKind::Text, "{role:?}");
        }
    }

    #[test]
    fn remote_engine_needs_consent_for_its_role_data_kind() {
        let mut policy = open_policy();
        policy.grant("p", DataKind::Text);
        for role in ALL_ROLES {
            let mut reg = EngineRegistry::new();
            let (registered, calls) = register(&mut reg, role, Some("p"), &policy);
            if role.data_kind() == DataKind::Text {
                assert_eq!(registered, Ok(()), "{role:?}");
                assert_eq!(get_and_call(&mut reg, role, &policy), Ok(()), "{role:?}");
                assert_eq!(calls.count(), 1, "{role:?}");
            } else {
                assert_eq!(
                    registered,
                    Err(denied(role, "p", DenyCause::NoConsent(DataKind::Audio))),
                    "{role:?}"
                );
            }
        }
    }

    #[test]
    fn consent_revoked_after_register_refuses_get_and_never_calls_engine() {
        for role in ALL_ROLES {
            let kind = role.data_kind();
            let mut policy = open_policy();
            policy.grant("p", kind);
            let mut reg = EngineRegistry::new();
            let (registered, calls) = register(&mut reg, role, Some("p"), &policy);
            assert_eq!(registered, Ok(()), "{role:?}");
            policy.revoke("p", kind);
            assert_eq!(
                get_and_call(&mut reg, role, &policy),
                Err(denied(role, "p", DenyCause::NoConsent(kind))),
                "{role:?}"
            );
            assert_eq!(calls.count(), 0, "{role:?}");
        }
    }

    #[test]
    fn local_only_switched_on_after_register_refuses_get_and_never_calls_engine() {
        for role in ALL_ROLES {
            let mut policy = open_policy();
            policy.grant("p", role.data_kind());
            let mut reg = EngineRegistry::new();
            let (registered, calls) = register(&mut reg, role, Some("p"), &policy);
            assert_eq!(registered, Ok(()), "{role:?}");
            policy.set_local_only(true);
            assert_eq!(
                get_and_call(&mut reg, role, &policy),
                Err(denied(role, "p", DenyCause::LocalOnly)),
                "{role:?}"
            );
            assert_eq!(calls.count(), 0, "{role:?}");
        }
    }

    #[test]
    fn empty_role_get_returns_not_loaded_for_every_role() {
        let policy = PrivacyPolicy::default();
        for role in ALL_ROLES {
            let mut reg = EngineRegistry::new();
            assert_eq!(
                get_and_call(&mut reg, role, &policy),
                Err(EngineError::NotLoaded(role))
            );
        }
    }

    #[test]
    fn refused_registration_keeps_the_old_engine() {
        let policy = PrivacyPolicy::default();
        for role in ALL_ROLES {
            let mut reg = EngineRegistry::new();
            let (first, first_calls) = register(&mut reg, role, None, &policy);
            assert_eq!(first, Ok(()), "{role:?}");
            let (second, second_calls) = register(&mut reg, role, Some("remote-p"), &policy);
            assert_eq!(
                second,
                Err(denied(role, "remote-p", DenyCause::LocalOnly)),
                "{role:?}"
            );
            assert_eq!(get_and_call(&mut reg, role, &policy), Ok(()), "{role:?}");
            assert_eq!(first_calls.count(), 1, "{role:?}");
            assert_eq!(second_calls.count(), 0, "{role:?}");
        }
    }

    #[test]
    fn second_local_engine_replaces_the_first() {
        let policy = PrivacyPolicy::default();
        for role in ALL_ROLES {
            let mut reg = EngineRegistry::new();
            let (_, first_calls) = register(&mut reg, role, None, &policy);
            let (second, second_calls) = register(&mut reg, role, None, &policy);
            assert_eq!(second, Ok(()), "{role:?}");
            assert_eq!(get_and_call(&mut reg, role, &policy), Ok(()), "{role:?}");
            assert_eq!(first_calls.count(), 0, "{role:?}");
            assert_eq!(second_calls.count(), 1, "{role:?}");
        }
    }

    #[test]
    fn model_host_denial_maps_to_a_backend_error_not_a_denied_engine() {
        let error = denial(
            EngineRole::Vad,
            &Provider::local("p"),
            EgressDenied::ModelHostNotAllowed("host.example.com".into()),
        );
        assert!(matches!(error, EngineError::Backend(_)), "{error:?}");
    }

    #[test]
    fn session_and_driver_refusals_map_to_a_backend_error() {
        for denied in [
            EgressDenied::MeetingRunning,
            EgressDenied::DriverHostNotAllowed("host.example.com".into()),
        ] {
            let error = denial(EngineRole::Vad, &Provider::local("p"), denied);
            assert!(matches!(error, EngineError::Backend(_)), "{error:?}");
        }
    }

    #[test]
    fn registry_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<EngineRegistry>();
    }
}
