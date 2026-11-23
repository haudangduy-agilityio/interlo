//! Scripted engines for tests. Built only for this crate's own tests and for
//! the cargo feature `mock`, never in the product build.
//!
//! One `Mock<V>` serves a role by the value it returns: `bool` for `Vad`,
//! `Recognition` for `Recognizer`, `String` for `Translator` and `Summarizer`,
//! `Vec<f32>` for `Synthesizer`, `SpeakerLabel` for `SpeakerId`. A mock is
//! local until `remote` makes it remote.

use crate::error::EngineError;
use crate::registry::EngineRole;
use crate::traits::{
    Engine, Recognition, Recognizer, SpeakerId, SpeakerLabel, Summarizer, Synthesizer, Translator,
    Vad,
};
use meeting_core::policy::Provider;
use meeting_core::protocol::Language;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Counts the calls that one mock received. Clone the handle before the mock
/// goes into a registry.
#[derive(Debug, Clone, Default)]
pub struct Calls(Arc<AtomicUsize>);

impl Calls {
    /// Number of calls so far.
    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// Roles in the order their mocks were called. Share one log between mocks to
/// check the order of a chain.
#[derive(Debug, Clone, Default)]
pub struct CallLog(Arc<Mutex<Vec<EngineRole>>>);

impl CallLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn roles(&self) -> Vec<EngineRole> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// A scripted engine that returns `value` from every call. See the module doc
/// for which `V` serves which role.
#[derive(Debug)]
pub struct Mock<V> {
    provider: Provider,
    value: V,
    calls: Calls,
    log: Option<CallLog>,
}

impl<V> Mock<V> {
    /// A local mock that returns `value` from every call.
    pub fn new(value: V) -> Self {
        Self {
            provider: Provider::local("mock-engine"),
            value,
            calls: Calls::default(),
            log: None,
        }
    }

    /// Make the mock remote, with this provider id.
    pub fn remote(mut self, provider_id: &str) -> Self {
        self.provider = Provider::remote(provider_id);
        self
    }

    /// Append the role of every call to `log`.
    pub fn with_log(mut self, log: &CallLog) -> Self {
        self.log = Some(log.clone());
        self
    }

    /// A handle on this mock's call counter; it stays valid after the mock
    /// moves into a registry.
    pub fn calls(&self) -> Calls {
        self.calls.clone()
    }

    fn record(&self, role: EngineRole) {
        self.calls.0.fetch_add(1, Ordering::SeqCst);
        if let Some(log) = &self.log {
            log.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(role);
        }
    }
}

impl<V: Send> Engine for Mock<V> {
    fn provider(&self) -> &Provider {
        &self.provider
    }
}

impl Vad for Mock<bool> {
    fn is_speech(&mut self, _frame: &[f32]) -> Result<bool, EngineError> {
        self.record(EngineRole::Vad);
        Ok(self.value)
    }
}

impl Recognizer for Mock<Recognition> {
    fn recognize(&mut self, _samples: &[f32]) -> Result<Recognition, EngineError> {
        self.record(EngineRole::Recognizer);
        Ok(self.value.clone())
    }
}

impl Translator for Mock<String> {
    fn translate(&mut self, _: &str, _: Language, _: Language) -> Result<String, EngineError> {
        self.record(EngineRole::Translator);
        Ok(self.value.clone())
    }
}

impl Synthesizer for Mock<Vec<f32>> {
    fn synthesize(&mut self, _: &str, _: Language) -> Result<Vec<f32>, EngineError> {
        self.record(EngineRole::Synthesizer);
        Ok(self.value.clone())
    }
}

impl SpeakerId for Mock<SpeakerLabel> {
    fn identify(&mut self, _samples: &[f32]) -> Result<SpeakerLabel, EngineError> {
        self.record(EngineRole::SpeakerId);
        Ok(self.value.clone())
    }
}

impl Summarizer for Mock<String> {
    fn summarize(&mut self, _: &str, _: Language) -> Result<String, EngineError> {
        self.record(EngineRole::Summarizer);
        Ok(self.value.clone())
    }
}
