//! The one privacy policy of the app. The session records its status in it, the
//! engines enter their registries through it and a model download reads it.

use std::sync::{Arc, PoisonError, RwLock};

use meeting_core::policy::PrivacyPolicy;
use meeting_core::protocol::SessionStatus;

use crate::commands::models::download_policy;

/// The policy that every part of the app shares.
pub type SharedPolicy = Arc<RwLock<PrivacyPolicy>>;

/// The policy at app launch: local-only, with the pinned model hosts.
pub fn shared_policy() -> SharedPolicy {
    Arc::new(RwLock::new(download_policy()))
}

/// Record the status of the session.
pub(super) fn record(policy: &SharedPolicy, status: SessionStatus) {
    policy
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .set_session_status(status);
}

/// The policy as it is now.
pub(super) fn current(policy: &SharedPolicy) -> PrivacyPolicy {
    policy
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}
