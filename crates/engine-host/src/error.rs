//! Error type shared by every engine trait.

use crate::registry::EngineRole;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("engine is not implemented yet")]
    NotImplemented,
    #[error("engine for role {0:?} is not loaded")]
    NotLoaded(EngineRole),
    #[error("engine failed: {0}")]
    Backend(String),
}
