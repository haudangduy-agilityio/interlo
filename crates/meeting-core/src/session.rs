//! Session lifecycle state machine (handoff MEETING_CORE_PROTOCOL.md).
//!
//! `idle -> preparing -> running <-> paused -> stopping -> completed`.
//! `preparing | running | stopping -> failed`. Recovery from `failed` is
//! explicit: only `failed -> idle` is allowed, and the caller must ask for it.

use crate::protocol::SessionStatus;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("illegal session transition {from:?} -> {to:?}")]
pub struct IllegalTransition {
    pub from: SessionStatus,
    pub to: SessionStatus,
}

pub fn can_transition(from: SessionStatus, to: SessionStatus) -> bool {
    use SessionStatus::*;
    matches!(
        (from, to),
        (Idle, Preparing)
            | (Preparing, Running)
            | (Running, Paused)
            | (Paused, Running)
            | (Running, Stopping)
            | (Paused, Stopping)
            | (Stopping, Completed)
            | (Preparing, Failed)
            | (Running, Failed)
            | (Stopping, Failed)
            | (Failed, Idle)
    )
}

pub fn transition(
    from: SessionStatus,
    to: SessionStatus,
) -> Result<SessionStatus, IllegalTransition> {
    if can_transition(from, to) {
        Ok(to)
    } else {
        Err(IllegalTransition { from, to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionStatus::*;

    #[test]
    fn happy_path_reaches_completed() {
        let mut s = Idle;
        for next in [Preparing, Running, Paused, Running, Stopping, Completed] {
            s = transition(s, next).expect("legal step");
        }
        assert_eq!(s, Completed);
    }

    #[test]
    fn completed_is_terminal() {
        assert!(transition(Completed, Idle).is_err());
        assert!(transition(Completed, Running).is_err());
    }

    #[test]
    fn failed_needs_explicit_reset_to_idle() {
        assert!(transition(Failed, Running).is_err());
        assert_eq!(transition(Failed, Idle), Ok(Idle));
    }

    #[test]
    fn cannot_skip_preparing() {
        assert!(transition(Idle, Running).is_err());
    }
}
