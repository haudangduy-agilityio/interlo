//! workspace-notes: the daily workspace. Notes, tasks, one tag per note,
//! search, Markdown export, and draft tasks from meeting action items.
//!
//! Owns: the note and task types, overdue and auto-delete rules, the draft
//! inbox (confirm or reject) and the Markdown export. All logic is pure: ids
//! and clocks come in as parameters.
//!
//! Must never: send notes or tasks to a network service. This crate has no
//! network code and no dependency that could open a connection.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DAY_MS: u64 = 86_400_000;

/// Longest accepted note text, in characters after trimming.
pub const MAX_NOTE_TEXT_CHARS: usize = 5_000;
/// Longest accepted tag, in characters after normalizing.
pub const MAX_TAG_CHARS: usize = 64;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NotesError {
    #[error("no draft with id {0}")]
    UnknownDraft(String),
    #[error("draft {0} was already confirmed or rejected")]
    DraftNotPending(String),
    #[error("{0:?} is not a valid tag")]
    InvalidTag(String),
    #[error("note text must not be empty")]
    EmptyText,
    #[error("note text must be at most {MAX_NOTE_TEXT_CHARS} characters")]
    TextTooLong,
    #[error("a tag must be at most {MAX_TAG_CHARS} characters")]
    TagTooLong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub text: String,
    /// At most one tag per note.
    pub tag: Option<String>,
    pub created_ms: u64,
}

impl Note {
    /// Builds a note from what the owner typed. The text is trimmed; blank text
    /// and text over `MAX_NOTE_TEXT_CHARS` are refused. No tag, a blank tag or a
    /// lone `#` mean "no tag". Any other tag is normalized with `normalize_tag`;
    /// one with inner whitespace or over `MAX_TAG_CHARS` is refused, so input is
    /// never dropped silently.
    pub fn from_user_input(
        id: impl Into<String>,
        text: &str,
        tag: Option<&str>,
        created_ms: u64,
    ) -> Result<Self, NotesError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(NotesError::EmptyText);
        }
        if text.chars().count() > MAX_NOTE_TEXT_CHARS {
            return Err(NotesError::TextTooLong);
        }
        let tag = match tag {
            Some(raw) if !raw.trim().trim_start_matches('#').trim().is_empty() => {
                let tag = normalize_tag(raw).ok_or_else(|| NotesError::InvalidTag(raw.into()))?;
                if tag.chars().count() > MAX_TAG_CHARS {
                    return Err(NotesError::TagTooLong);
                }
                Some(tag)
            }
            _ => None,
        };
        Ok(Self {
            id: id.into(),
            text: text.to_string(),
            tag,
            created_ms,
        })
    }

    /// Builds a note. A tag is normalized with `normalize_tag`; a tag that
    /// normalizes to nothing is an error, not a silent drop.
    pub fn new(
        id: impl Into<String>,
        text: impl Into<String>,
        tag: Option<&str>,
        created_ms: u64,
    ) -> Result<Self, NotesError> {
        let tag = match tag {
            None => None,
            Some(raw) => {
                Some(normalize_tag(raw).ok_or_else(|| NotesError::InvalidTag(raw.into()))?)
            }
        };
        Ok(Self {
            id: id.into(),
            text: text.into(),
            tag,
            created_ms,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub due_ms: Option<u64>,
    pub status: TaskStatus,
}

/// Lowercase, no leading `#`, no inner whitespace. `None` when nothing is left.
pub fn normalize_tag(raw: &str) -> Option<String> {
    let t = raw.trim().trim_start_matches('#').trim();
    if t.is_empty() || t.chars().any(char::is_whitespace) {
        return None;
    }
    Some(t.to_lowercase())
}

/// An open task is overdue when its due time is before `now_ms`.
pub fn is_overdue(task: &Task, now_ms: u64) -> bool {
    task.status == TaskStatus::Open && task.due_ms.is_some_and(|due| due < now_ms)
}

/// Case-insensitive substring search over note text and tag.
/// A blank query matches nothing.
pub fn search_notes<'a>(notes: &'a [Note], query: &str) -> Vec<&'a Note> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    notes
        .iter()
        .filter(|n| {
            n.text.to_lowercase().contains(&q)
                || n.tag
                    .as_deref()
                    .is_some_and(|t| t.to_lowercase().contains(&q))
        })
        .collect()
}

/// An action item found in a meeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionItem {
    pub text: String,
    pub due_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftState {
    Pending,
    Confirmed,
    Rejected,
}

/// A task suggestion. It is not a task until the owner confirms it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftTask {
    pub id: String,
    pub meeting_id: String,
    pub title: String,
    pub due_ms: Option<u64>,
    state: DraftState,
}

impl DraftTask {
    pub fn state(&self) -> DraftState {
        self.state
    }
}

/// Holds drafts. `confirm` is the only way to get a `Task` out of a draft.
#[derive(Debug, Default)]
pub struct DraftInbox {
    drafts: Vec<DraftTask>,
}

impl DraftInbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one pending draft per non-blank action item. `next_id` supplies ids.
    pub fn propose(
        &mut self,
        meeting_id: &str,
        items: &[ActionItem],
        mut next_id: impl FnMut() -> String,
    ) {
        for item in items.iter().filter(|i| !i.text.trim().is_empty()) {
            self.drafts.push(DraftTask {
                id: next_id(),
                meeting_id: meeting_id.to_string(),
                title: item.text.trim().to_string(),
                due_ms: item.due_ms,
                state: DraftState::Pending,
            });
        }
    }

    pub fn pending(&self) -> impl Iterator<Item = &DraftTask> {
        self.drafts
            .iter()
            .filter(|d| d.state == DraftState::Pending)
    }

    fn pending_mut(&mut self, id: &str) -> Result<&mut DraftTask, NotesError> {
        let d = self
            .drafts
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| NotesError::UnknownDraft(id.to_string()))?;
        if d.state != DraftState::Pending {
            return Err(NotesError::DraftNotPending(id.to_string()));
        }
        Ok(d)
    }

    /// The owner confirmed: turn the draft into an open task with the same id.
    pub fn confirm(&mut self, id: &str) -> Result<Task, NotesError> {
        let d = self.pending_mut(id)?;
        d.state = DraftState::Confirmed;
        Ok(Task {
            id: d.id.clone(),
            title: d.title.clone(),
            due_ms: d.due_ms,
            status: TaskStatus::Open,
        })
    }

    /// The owner rejected: the draft never becomes a task.
    pub fn reject(&mut self, id: &str) -> Result<(), NotesError> {
        self.pending_mut(id)?.state = DraftState::Rejected;
        Ok(())
    }
}

/// Auto-delete of old notes. Off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoDelete {
    pub enabled: bool,
    pub days: u32,
}

impl Default for AutoDelete {
    fn default() -> Self {
        Self {
            enabled: false,
            days: 30,
        }
    }
}

/// Ids of notes older than `days`. Empty when auto-delete is off.
pub fn notes_to_delete(auto: &AutoDelete, notes: &[Note], now_ms: u64) -> Vec<String> {
    if !auto.enabled {
        return Vec::new();
    }
    let max_age = u64::from(auto.days).saturating_mul(DAY_MS);
    notes
        .iter()
        .filter(|n| now_ms.saturating_sub(n.created_ms) > max_age)
        .map(|n| n.id.clone())
        .collect()
}

/// `YYYY-MM-DD` in UTC for a unix time in milliseconds.
pub fn format_date_utc(ms: u64) -> String {
    format_date_with_offset(ms, 0)
}

/// `YYYY-MM-DD` for a unix time in milliseconds, shifted by a time zone
/// offset in minutes east of UTC (for example 420 for UTC+7). The caller reads
/// the offset from the system; this crate has no clock or zone data.
pub fn format_date_with_offset(ms: u64, utc_offset_minutes: i32) -> String {
    let local_ms = i64::try_from(ms)
        .unwrap_or(i64::MAX)
        .saturating_add(i64::from(utc_offset_minutes) * 60_000);
    // Civil-from-days over the whole i64 range of days.
    let days = local_ms.div_euclid(DAY_MS as i64) + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Turns every kind of line break into `\n`, so line handling sees them all.
fn unify_line_breaks(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace(['\r', '\u{0085}', '\u{2028}', '\u{2029}'], "\n")
}

/// Prefixes a backslash when the line would start a list item, heading or
/// quote in Markdown, so the text stays text.
fn escape_line_start(line: &str) -> String {
    let body = line.trim_start();
    let mut chars = body.chars();
    let starts_block = match chars.next() {
        Some('-' | '*' | '+' | '#' | '>') => true,
        Some(c) if c.is_ascii_digit() => {
            let rest = body.trim_start_matches(|c: char| c.is_ascii_digit());
            rest.starts_with('.') || rest.starts_with(')')
        }
        _ => false,
    };
    if starts_block {
        let indent = &line[..line.len() - body.len()];
        format!("{indent}\\{body}")
    } else {
        line.to_string()
    }
}

/// One line of text for a task title: line breaks become spaces.
fn single_line(text: &str) -> String {
    unify_line_breaks(text).replace('\n', " ")
}

/// Markdown for the notes (oldest first) and the tasks (given order). Dates
/// are local dates for `utc_offset_minutes`. Text from a note or task never
/// starts a new list item, task or heading: a title is one line, and a line
/// that begins with a list marker is escaped with a backslash.
pub fn export_markdown(notes: &[Note], tasks: &[Task], utc_offset_minutes: i32) -> String {
    let mut out = String::from("# Notes\n\n");
    if notes.is_empty() {
        out.push_str("_No notes._\n");
    }
    let mut sorted: Vec<&Note> = notes.iter().collect();
    sorted.sort_by_key(|n| n.created_ms);
    for n in sorted {
        let text = unify_line_breaks(&n.text);
        let mut lines = text.lines();
        out.push_str(&format!(
            "- {} {}",
            format_date_with_offset(n.created_ms, utc_offset_minutes),
            lines.next().unwrap_or("")
        ));
        if let Some(tag) = n.tag.as_deref().and_then(normalize_tag) {
            out.push_str(&format!(" #{tag}"));
        }
        out.push('\n');
        for rest in lines {
            out.push_str(&format!("  {}\n", escape_line_start(rest)));
        }
    }
    out.push_str("\n# Tasks\n\n");
    if tasks.is_empty() {
        out.push_str("_No tasks._\n");
    }
    for t in tasks {
        let mark = if t.status == TaskStatus::Done {
            "x"
        } else {
            " "
        };
        out.push_str(&format!(
            "- [{mark}] {}",
            escape_line_start(&single_line(&t.title))
        ));
        if let Some(due) = t.due_ms {
            out.push_str(&format!(
                " (due {})",
                format_date_with_offset(due, utc_offset_minutes)
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, text: &str, tag: Option<&str>, created_ms: u64) -> Note {
        Note {
            id: id.into(),
            text: text.into(),
            tag: tag.map(String::from),
            created_ms,
        }
    }

    fn task(id: &str, due_ms: Option<u64>, status: TaskStatus) -> Task {
        Task {
            id: id.into(),
            title: format!("task {id}"),
            due_ms,
            status,
        }
    }

    fn counter() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("d{n}")
        }
    }

    fn items() -> Vec<ActionItem> {
        vec![
            ActionItem {
                text: "Send the report".into(),
                due_ms: Some(5_000),
            },
            ActionItem {
                text: "   ".into(),
                due_ms: None,
            },
            ActionItem {
                text: "Book a room".into(),
                due_ms: None,
            },
        ]
    }

    #[test]
    fn overdue_rules() {
        let open = task("a", Some(100), TaskStatus::Open);
        assert!(is_overdue(&open, 101));
        assert!(!is_overdue(&open, 100), "due now is not overdue");
        assert!(!is_overdue(&open, 99));
        assert!(!is_overdue(&task("b", Some(100), TaskStatus::Done), 1_000));
        assert!(!is_overdue(&task("c", None, TaskStatus::Open), u64::MAX));
    }

    #[test]
    fn tags_are_normalized() {
        assert_eq!(normalize_tag("  #Work "), Some("work".into()));
        assert_eq!(normalize_tag("home"), Some("home".into()));
        assert_eq!(normalize_tag("#"), None);
        assert_eq!(normalize_tag("two words"), None);
        assert_eq!(normalize_tag(""), None);
    }

    #[test]
    fn search_matches_text_and_tag_without_case() {
        let notes = vec![
            note("1", "Buy Oat Milk", None, 1),
            note("2", "call dentist", Some("health"), 2),
        ];
        assert_eq!(search_notes(&notes, "oat milk")[0].id, "1");
        assert_eq!(search_notes(&notes, "HEALTH")[0].id, "2");
        assert!(search_notes(&notes, "zzz").is_empty());
        assert!(search_notes(&notes, "  ").is_empty());
    }

    #[test]
    fn drafts_are_not_tasks_until_confirmed() {
        let mut inbox = DraftInbox::new();
        inbox.propose("m1", &items(), counter());
        // Blank item skipped.
        assert_eq!(inbox.pending().count(), 2);
        assert!(inbox.pending().all(|d| d.state() == DraftState::Pending));
        // Nothing turned into a task by itself: only confirm returns a Task.
        let t = inbox.confirm("d1").unwrap();
        assert_eq!(t.title, "Send the report");
        assert_eq!(t.due_ms, Some(5_000));
        assert_eq!(t.status, TaskStatus::Open);
        assert_eq!(inbox.pending().count(), 1);
    }

    #[test]
    fn rejected_draft_never_becomes_a_task() {
        let mut inbox = DraftInbox::new();
        inbox.propose("m1", &items(), counter());
        inbox.reject("d1").unwrap();
        assert_eq!(
            inbox.confirm("d1"),
            Err(NotesError::DraftNotPending("d1".into()))
        );
        assert_eq!(inbox.pending().count(), 1);
    }

    #[test]
    fn a_draft_is_confirmed_once() {
        let mut inbox = DraftInbox::new();
        inbox.propose("m1", &items(), counter());
        inbox.confirm("d1").unwrap();
        assert_eq!(
            inbox.confirm("d1"),
            Err(NotesError::DraftNotPending("d1".into()))
        );
        assert_eq!(
            inbox.reject("d1"),
            Err(NotesError::DraftNotPending("d1".into()))
        );
        assert_eq!(
            inbox.confirm("nope"),
            Err(NotesError::UnknownDraft("nope".into()))
        );
    }

    #[test]
    fn auto_delete_is_off_by_default_and_deletes_nothing() {
        let auto = AutoDelete::default();
        assert!(!auto.enabled);
        let notes = vec![note("old", "x", None, 0)];
        assert!(notes_to_delete(&auto, &notes, 10_000 * DAY_MS).is_empty());
    }

    #[test]
    fn auto_delete_picks_notes_older_than_the_window() {
        let auto = AutoDelete {
            enabled: true,
            days: 7,
        };
        let now = 100 * DAY_MS;
        let notes = vec![
            note("old", "x", None, now - 8 * DAY_MS),
            note("edge", "x", None, now - 7 * DAY_MS),
            note("new", "x", None, now - DAY_MS),
            note("future", "x", None, now + DAY_MS),
        ];
        assert_eq!(notes_to_delete(&auto, &notes, now), vec!["old".to_string()]);
    }

    #[test]
    fn dates_are_formatted_in_utc() {
        assert_eq!(format_date_utc(0), "1970-01-01");
        assert_eq!(format_date_utc(1_700_000_000_000), "2023-11-14");
        // Leap day.
        assert_eq!(format_date_utc(1_709_164_800_000), "2024-02-29");
        assert_eq!(format_date_utc(1_709_251_199_999), "2024-02-29");
        assert_eq!(format_date_utc(1_709_251_200_000), "2024-03-01");
    }

    #[test]
    fn markdown_export_lists_notes_then_tasks() {
        let notes = vec![
            note("2", "second\nsecond line", Some("work"), 1_700_000_000_000),
            note("1", "first", None, 0),
        ];
        let tasks = vec![
            task("a", Some(1_700_000_000_000), TaskStatus::Open),
            task("b", None, TaskStatus::Done),
        ];
        let md = export_markdown(&notes, &tasks, 0);
        let want = "# Notes\n\n\
- 1970-01-01 first\n\
- 2023-11-14 second #work\n  second line\n\
\n# Tasks\n\n\
- [ ] task a (due 2023-11-14)\n\
- [x] task b\n";
        assert_eq!(md, want);
    }

    #[test]
    fn markdown_export_of_nothing_says_so() {
        let md = export_markdown(&[], &[], 0);
        assert!(md.contains("_No notes._"));
        assert!(md.contains("_No tasks._"));
    }

    /// 2023-11-14 20:50 UTC.
    const NOV_14_2050_UTC_MS: u64 = 1_699_995_000_000;

    #[test]
    fn export_uses_the_local_date_from_the_offset() {
        let notes = vec![note("1", "late note", None, NOV_14_2050_UTC_MS)];
        let utc = export_markdown(&notes, &[], 0);
        assert!(utc.contains("- 2023-11-14 late note"), "{utc}");
        // UTC+7 at 20:50 UTC is 03:50 the next day.
        let hcm = export_markdown(&notes, &[], 420);
        assert!(hcm.contains("- 2023-11-15 late note"), "{hcm}");
        // UTC-8 at 20:50 UTC is 12:50 the same day.
        let la = export_markdown(&notes, &[], -480);
        assert!(la.contains("- 2023-11-14 late note"), "{la}");
    }

    #[test]
    fn export_applies_the_offset_to_task_due_dates_too() {
        let tasks = vec![task("a", Some(NOV_14_2050_UTC_MS), TaskStatus::Open)];
        assert!(export_markdown(&[], &tasks, 0).contains("(due 2023-11-14)"));
        assert!(export_markdown(&[], &tasks, 420).contains("(due 2023-11-15)"));
    }

    #[test]
    fn local_date_handles_offsets_around_the_epoch() {
        assert_eq!(format_date_with_offset(0, 0), "1970-01-01");
        assert_eq!(format_date_with_offset(0, -1), "1969-12-31");
        assert_eq!(format_date_with_offset(DAY_MS - 1, 1), "1970-01-02");
        // A huge time and a huge offset must not wrap or panic.
        let far = format_date_with_offset(u64::MAX, i32::MIN);
        assert!(
            far.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "{far}"
        );
        assert!(far.matches('-').count() == 2, "{far}");
    }

    #[test]
    fn a_task_title_cannot_forge_extra_task_lines() {
        let mut t = task("a", None, TaskStatus::Open);
        t.title = "real\n- [x] forged task\r\n- [ ] another".into();
        let md = export_markdown(&[], &[t], 0);
        let task_lines: Vec<&str> = md
            .lines()
            .skip_while(|l| *l != "# Tasks")
            .filter(|l| l.starts_with("- ["))
            .collect();
        assert_eq!(task_lines.len(), 1, "{md}");
        assert!(task_lines[0].starts_with("- [ ] real"));
    }

    #[test]
    fn a_task_title_with_a_leading_list_marker_is_escaped() {
        for title in [
            "- [x] done already",
            "* bullet",
            "+ plus",
            "# heading",
            "> quote",
            "1. one",
            "2) two",
        ] {
            let mut t = task("a", None, TaskStatus::Open);
            t.title = title.into();
            let md = export_markdown(&[], &[t], 0);
            assert!(
                md.contains(&format!("- [ ] \\{title}\n")),
                "{title} was not escaped in {md}"
            );
        }
    }

    #[test]
    fn a_plain_task_title_is_unchanged() {
        let mut t = task("a", None, TaskStatus::Open);
        t.title = "Buy milk - 2 litres".into();
        assert!(export_markdown(&[], &[t], 0).contains("- [ ] Buy milk - 2 litres\n"));
    }

    #[test]
    fn a_note_cannot_forge_list_items_or_tasks_on_later_lines() {
        let notes = vec![note(
            "1",
            "first\n- [x] forged\n* bullet\n# Tasks\r\n1. one",
            None,
            0,
        )];
        let md = export_markdown(&notes, &[], 0);
        let notes_part = md.split("\n# Tasks\n").next().unwrap();
        for line in notes_part.lines().skip(2) {
            // Every note line is either the dated item or indented continuation.
            assert!(
                line.starts_with("- 1970-01-01 ") || line.starts_with("  "),
                "line {line:?} escapes the note in {md}"
            );
        }
        assert_eq!(md.matches("\n# Tasks\n").count(), 1, "{md}");
        assert!(md.contains("  \\- [x] forged\n"));
        assert!(md.contains("  \\* bullet\n"));
        assert!(md.contains("  \\# Tasks\n"));
        assert!(md.contains("  \\1. one\n"));
    }

    #[test]
    fn note_new_normalizes_the_tag() {
        let n = Note::new("1", "x", Some("  #Work "), 5).unwrap();
        assert_eq!(n.tag, Some("work".into()));
        assert_eq!(Note::new("1", "x", None, 5).unwrap().tag, None);
    }

    #[test]
    fn note_new_refuses_a_tag_that_cannot_be_normalized() {
        for bad in ["", "#", "two words", "tab\there"] {
            assert_eq!(
                Note::new("1", "x", Some(bad), 5),
                Err(NotesError::InvalidTag(bad.into())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn user_input_is_trimmed_and_checked() {
        let n = Note::from_user_input("n1", "  hello ", Some("  #Work "), 5).unwrap();
        assert_eq!(
            (n.text.as_str(), n.tag.as_deref(), n.created_ms),
            ("hello", Some("work"), 5)
        );
        for blank in ["", "   ", "\n\t"] {
            assert_eq!(
                Note::from_user_input("n", blank, None, 1),
                Err(NotesError::EmptyText)
            );
        }
    }

    #[test]
    fn user_input_text_limit_counts_characters_not_bytes() {
        assert!(Note::from_user_input("n", &"a".repeat(MAX_NOTE_TEXT_CHARS), None, 1).is_ok());
        assert_eq!(
            Note::from_user_input("n", &"a".repeat(MAX_NOTE_TEXT_CHARS + 1), None, 1),
            Err(NotesError::TextTooLong)
        );
        assert!(
            Note::from_user_input("n", &"\u{1F600}".repeat(MAX_NOTE_TEXT_CHARS), None, 1).is_ok()
        );
        assert!(
            Note::from_user_input("n", &"\u{1F600}".repeat(MAX_NOTE_TEXT_CHARS + 1), None, 1)
                .is_err()
        );
    }

    #[test]
    fn user_input_tag_rules() {
        for none in [None, Some(""), Some(" "), Some("#"), Some("###")] {
            assert_eq!(
                Note::from_user_input("n", "x", none, 1).unwrap().tag,
                None,
                "{none:?}"
            );
        }
        assert_eq!(
            Note::from_user_input("n", "x", Some("##Two"), 1)
                .unwrap()
                .tag
                .as_deref(),
            Some("two")
        );
        for bad in ["two words", "a\tb", "# a b"] {
            assert_eq!(
                Note::from_user_input("n", "x", Some(bad), 1),
                Err(NotesError::InvalidTag(bad.into())),
                "tag {bad:?}"
            );
        }
        assert!(Note::from_user_input("n", "x", Some(&"a".repeat(MAX_TAG_CHARS)), 1).is_ok());
        assert_eq!(
            Note::from_user_input("n", "x", Some(&"a".repeat(MAX_TAG_CHARS + 1)), 1),
            Err(NotesError::TagTooLong)
        );
    }
}
