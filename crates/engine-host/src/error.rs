//! Error type shared by every engine trait.

use crate::registry::EngineRole;
use meeting_core::protocol::DataKind;

/// Why the privacy policy refused an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyCause {
    /// Local-only mode is on and the engine is remote.
    LocalOnly,
    /// No consent for this provider and this kind of data.
    NoConsent(DataKind),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("engine is not implemented yet")]
    NotImplemented,
    #[error("engine for role {0:?} is not loaded")]
    NotLoaded(EngineRole),
    /// The privacy policy refused the engine of `role`. `provider` is the
    /// provider id only; it never holds audio or text.
    #[error("privacy policy refused the {role:?} engine of provider {provider}: {cause:?}")]
    Denied {
        role: EngineRole,
        provider: String,
        cause: DenyCause,
    },
    #[error("engine failed: {0}")]
    Backend(String),
}
