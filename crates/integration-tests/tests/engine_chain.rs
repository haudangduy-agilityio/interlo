//! Mock engines drive a Vietnamese span through the four chain roles, using
//! only the registry's `get_*` calls.

use engine_host::mock::{CallLog, Mock};
use engine_host::{EngineError, EngineRegistry, EngineRole, Recognition};
use meeting_core::policy::PrivacyPolicy;
use meeting_core::protocol::Language;

const VOICE_SAMPLES: [f32; 3] = [0.1, -0.2, 0.3];

fn chain_registry(log: &CallLog, policy: &PrivacyPolicy) -> Result<EngineRegistry, EngineError> {
    let mut reg = EngineRegistry::new();
    reg.set_vad(Box::new(Mock::new(true).with_log(log)), policy)?;
    reg.set_recognizer(
        Box::new(
            Mock::new(Recognition {
                text: "xin chao".into(),
                detected: Language::Vi,
            })
            .with_log(log),
        ),
        policy,
    )?;
    reg.set_translator(
        Box::new(Mock::new(String::from("hello")).with_log(log)),
        policy,
    )?;
    reg.set_synthesizer(
        Box::new(Mock::new(VOICE_SAMPLES.to_vec()).with_log(log)),
        policy,
    )?;
    Ok(reg)
}

/// One span through voice activity detection, recognition, translation
/// (Vietnamese to English) and the English voice. Returns the voice samples.
fn run_span(reg: &mut EngineRegistry, policy: &PrivacyPolicy) -> Result<Vec<f32>, EngineError> {
    let span = [0.0_f32; 512];
    if !reg.get_vad(policy)?.is_speech(&span)? {
        return Ok(Vec::new());
    }
    let heard = reg.get_recognizer(policy)?.recognize(&span)?;
    let english =
        reg.get_translator(policy)?
            .translate(&heard.text, heard.detected, Language::En)?;
    reg.get_synthesizer(policy)?
        .synthesize(&english, Language::En)
}

#[test]
fn vietnamese_span_runs_through_four_mock_roles_in_order() {
    let policy = PrivacyPolicy::default();
    let log = CallLog::new();
    let mut reg = chain_registry(&log, &policy).expect("local mocks pass the default policy");
    assert_eq!(
        run_span(&mut reg, &policy).expect("chain runs"),
        VOICE_SAMPLES.to_vec()
    );
    assert_eq!(
        log.roles(),
        vec![
            EngineRole::Vad,
            EngineRole::Recognizer,
            EngineRole::Translator,
            EngineRole::Synthesizer
        ]
    );
    assert_eq!(
        reg.missing_roles(),
        vec![EngineRole::SpeakerId, EngineRole::Summarizer]
    );
}

#[test]
fn span_stops_with_not_loaded_when_a_chain_role_is_missing() {
    let policy = PrivacyPolicy::default();
    let mut reg = EngineRegistry::new();
    assert_eq!(
        run_span(&mut reg, &policy),
        Err(EngineError::NotLoaded(EngineRole::Vad))
    );
}
