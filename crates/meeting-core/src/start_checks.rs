//! Pure input rules for the per-meeting start checks (layer 2, ADR 0009).
//!
//! The Tauri command layer owns the checklist itself. This module owns the
//! shape of the one free-text value that reaches it from the webview, the
//! meeting app id, so the rule is testable without a Tauri runtime.

use thiserror::Error;

/// Most characters a meeting app id may have. Starting value: real bundle
/// identifiers are far shorter, and the id is shown in the UI and logs.
pub const MAX_MEETING_APP_ID_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MeetingAppIdError {
    #[error("meeting app id is empty")]
    Empty,
    #[error("meeting app id has {len} characters; the limit is {max}")]
    TooLong { len: usize, max: usize },
    #[error("meeting app id contains a control character")]
    ControlCharacter,
}

/// Accepts a non-empty id of at most `MAX_MEETING_APP_ID_CHARS` characters
/// with no control character (newline, tab, NUL, DEL and the other Unicode
/// control characters). The length counts characters, not bytes. Surrounding
/// spaces are not control characters; an id of only spaces is empty.
pub fn check_meeting_app_id(id: &str) -> Result<(), MeetingAppIdError> {
    if id.trim().is_empty() {
        return Err(MeetingAppIdError::Empty);
    }
    if id.chars().any(char::is_control) {
        return Err(MeetingAppIdError::ControlCharacter);
    }
    let len = id.chars().count();
    if len > MAX_MEETING_APP_ID_CHARS {
        return Err(MeetingAppIdError::TooLong {
            len,
            max: MAX_MEETING_APP_ID_CHARS,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_bundle_identifier_is_accepted() {
        assert_eq!(check_meeting_app_id("sample-meeting-app"), Ok(()));
        assert_eq!(check_meeting_app_id("com.example.Meet"), Ok(()));
    }

    #[test]
    fn an_empty_or_blank_id_is_refused() {
        for id in ["", " ", "   "] {
            assert_eq!(
                check_meeting_app_id(id),
                Err(MeetingAppIdError::Empty),
                "{id:?}"
            );
        }
    }

    #[test]
    fn the_length_limit_is_64_characters_inclusive() {
        let at_limit = "a".repeat(MAX_MEETING_APP_ID_CHARS);
        assert_eq!(check_meeting_app_id(&at_limit), Ok(()));
        let over = "a".repeat(MAX_MEETING_APP_ID_CHARS + 1);
        assert_eq!(
            check_meeting_app_id(&over),
            Err(MeetingAppIdError::TooLong { len: 65, max: 64 })
        );
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        // 64 two-byte characters: 128 bytes, still inside the limit.
        let wide = "é".repeat(MAX_MEETING_APP_ID_CHARS);
        assert_eq!(check_meeting_app_id(&wide), Ok(()));
        let wide_over = "é".repeat(MAX_MEETING_APP_ID_CHARS + 1);
        assert!(matches!(
            check_meeting_app_id(&wide_over),
            Err(MeetingAppIdError::TooLong { len: 65, .. })
        ));
    }

    #[test]
    fn control_characters_are_refused_wherever_they_sit() {
        for id in [
            "app\n",
            "\napp",
            "ap\tp",
            "a\0b",
            "a\u{7f}b",
            "a\u{85}b",
            "a\r\nb",
            "a\u{1b}[31mb",
        ] {
            assert_eq!(
                check_meeting_app_id(id),
                Err(MeetingAppIdError::ControlCharacter),
                "{id:?}"
            );
        }
    }

    #[test]
    fn a_control_character_wins_over_the_length_error() {
        let both = format!("{}\n", "a".repeat(100));
        assert_eq!(
            check_meeting_app_id(&both),
            Err(MeetingAppIdError::ControlCharacter)
        );
    }

    #[test]
    fn spaces_inside_an_id_are_allowed() {
        assert_eq!(check_meeting_app_id("My Meeting App"), Ok(()));
    }
}
