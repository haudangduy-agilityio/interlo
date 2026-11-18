use crate::clock::{now_ms, unique_id};
use workspace_notes::normalize_tag;

use crate::ipc::{AddNoteArgs, IpcError, IpcResult, Note};

/// Longest accepted note text, in characters after trimming.
pub const MAX_NOTE_TEXT_CHARS: usize = 5_000;
/// Longest accepted tag, in characters after normalizing.
pub const MAX_TAG_CHARS: usize = 64;

/// No tag, a blank tag or a lone `#` mean "no tag". A tag with inner whitespace
/// or over `MAX_TAG_CHARS` is refused, so user input is never dropped silently.
fn parse_tag(raw: Option<String>) -> IpcResult<Option<String>> {
    let Some(raw) = raw else { return Ok(None) };
    if raw.trim().trim_start_matches('#').trim().is_empty() {
        return Ok(None);
    }
    let Some(tag) = normalize_tag(&raw) else {
        return Err(IpcError::invalid_input("Tag must be a single word."));
    };
    if tag.chars().count() > MAX_TAG_CHARS {
        return Err(IpcError::invalid_input(format!(
            "Tag must be at most {MAX_TAG_CHARS} characters."
        )));
    }
    Ok(Some(tag))
}

/// Sample rows, newest first. Tests and a `demo-data` build only.
#[cfg(any(test, feature = "demo-data"))]
pub fn sample_notes() -> Vec<Note> {
    vec![
        Note {
            id: "note-2".to_string(),
            text: "Ask about the pricing tiers.".to_string(),
            tag: None,
            created_at_ms: 1_763_000_900_000,
        },
        Note {
            id: "note-1".to_string(),
            text: "Send the revised timeline.".to_string(),
            tag: Some("action".to_string()),
            created_at_ms: 1_763_000_600_000,
        },
    ]
}

/// Order rows newest first; the order the UI relies on.
pub fn newest_first(mut rows: Vec<Note>) -> Vec<Note> {
    rows.sort_by_key(|n| std::cmp::Reverse(n.created_at_ms));
    rows
}

/// Build a note from user input. Blank text, text over `MAX_NOTE_TEXT_CHARS`
/// and a bad tag are refused with `INVALID_INPUT`.
pub fn make_note(text: &str, tag: Option<String>, now_ms: u64) -> IpcResult<Note> {
    let text = text.trim();
    if text.is_empty() {
        return Err(IpcError::invalid_input("Note text must not be empty."));
    }
    if text.chars().count() > MAX_NOTE_TEXT_CHARS {
        return Err(IpcError::invalid_input(format!(
            "Note text must be at most {MAX_NOTE_TEXT_CHARS} characters."
        )));
    }
    let tag = parse_tag(tag)?;
    Ok(Note {
        id: unique_id("note", now_ms),
        text: text.to_string(),
        tag,
        created_at_ms: now_ms,
    })
}

/// The notes the owner sees. There is no note store yet, so a default build
/// answers `NOT_IMPLEMENTED` instead of inventing rows.
#[cfg(any(test, feature = "demo-data"))]
pub fn current_notes() -> IpcResult<Vec<Note>> {
    Ok(newest_first(sample_notes()))
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn current_notes() -> IpcResult<Vec<Note>> {
    Err(IpcError::not_implemented("Notes"))
}

/// Validate and store one note. The input is checked first, so bad input is
/// `INVALID_INPUT` in every build. There is no note store yet, so a default build
/// answers `NOT_IMPLEMENTED` and never returns a note it did not keep.
#[cfg(any(test, feature = "demo-data"))]
pub fn save_note(text: &str, tag: Option<String>, now_ms: u64) -> IpcResult<Note> {
    make_note(text, tag, now_ms)
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn save_note(text: &str, tag: Option<String>, now_ms: u64) -> IpcResult<Note> {
    make_note(text, tag, now_ms)?;
    Err(IpcError::not_implemented("Saving notes"))
}

#[tauri::command]
pub async fn list_notes() -> IpcResult<Vec<Note>> {
    current_notes()
}

#[tauri::command]
pub async fn add_note(text: String, tag: Option<String>) -> IpcResult<Note> {
    let args = AddNoteArgs { text, tag };
    save_note(&args.text, args.tag, now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ErrorCode;

    #[test]
    fn the_tag_rule_is_the_one_in_workspace_notes_not_a_copy() {
        let source = include_str!("notes.rs");
        assert!(!source.contains(concat!("fn normalize_", "tag(")));
        assert!(source.contains("use workspace_notes::normalize_tag;"));
    }

    #[test]
    fn empty_text_is_invalid_input() {
        for text in ["", "   ", "\n\t"] {
            let err = make_note(text, None, 1).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput);
        }
    }

    #[test]
    fn newest_first_orders_unsorted_rows() {
        let mut rows = sample_notes();
        rows.reverse();
        let sorted = newest_first(rows);
        assert!(sorted
            .windows(2)
            .all(|w| w[0].created_at_ms > w[1].created_at_ms));
    }

    #[test]
    fn sample_notes_are_newest_first() {
        let rows = sample_notes();
        assert!(rows
            .windows(2)
            .all(|w| w[0].created_at_ms > w[1].created_at_ms));
    }

    #[test]
    fn text_longer_than_5000_chars_is_invalid_input() {
        assert!(make_note(&"a".repeat(5000), None, 1).is_ok());
        let err = make_note(&"a".repeat(5001), None, 1).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn tag_is_normalized_and_limited_to_64_chars() {
        let note = make_note("x", Some("  #Work ".into()), 1).unwrap();
        assert_eq!(note.tag.as_deref(), Some("work"));
        assert!(make_note("x", Some("a".repeat(64)), 1).is_ok());
        let err = make_note("x", Some("a".repeat(65)), 1).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn note_uses_camel_case_names() {
        let note = make_note("  hello ", Some("idea".into()), 42).unwrap();
        let json = serde_json::to_value(&note).unwrap();
        assert_eq!(json["text"], "hello");
        assert_eq!(json["tag"], "idea");
        assert_eq!(json["createdAtMs"], 42);
        let untagged = make_note("x", Some(" ".into()), 1).unwrap();
        assert!(untagged.tag.is_none());
    }

    #[test]
    fn ids_are_unique_even_within_one_millisecond() {
        let ids: std::collections::HashSet<String> = (0..1000)
            .map(|_| make_note("x", None, 7).unwrap().id)
            .collect();
        assert_eq!(ids.len(), 1000);
    }

    #[test]
    fn tag_follows_the_workspace_notes_rule() {
        assert_eq!(make_note("x", Some("#".into()), 1).unwrap().tag, None);
        assert_eq!(make_note("x", Some("###".into()), 1).unwrap().tag, None);
        assert_eq!(make_note("x", None, 1).unwrap().tag, None);
        assert_eq!(
            make_note("x", Some("##Two".into()), 1)
                .unwrap()
                .tag
                .as_deref(),
            Some("two")
        );
        for bad in ["two words", "a\tb", "# a b"] {
            let err = make_note("x", Some(bad.into()), 1).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "tag {bad:?}");
        }
    }

    #[test]
    fn text_limit_counts_characters_not_bytes() {
        assert!(make_note(&"\u{1F600}".repeat(5000), None, 1).is_ok());
        assert!(make_note(&"\u{1F600}".repeat(5001), None, 1).is_err());
    }
}
