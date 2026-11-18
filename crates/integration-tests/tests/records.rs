//! Records: store, summary citation check and notes export.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use meeting_core::protocol::{Language, SegmentStatus};
use store::{Store, Utterance};
use summary_jobs::{check_citations, decide_notes, NotesDecision, Point, RejectReason};
use workspace_notes::{export_markdown, Note, Task, TaskStatus, DAY_MS};

fn ready_store() -> Store {
    let s = Store::open_in_memory().expect("open");
    s.migrate().expect("migrate");
    s
}

fn count(s: &Store, table: &str) -> i64 {
    s.conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

fn final_line(id: &str, meeting_id: &str, start_ms: u64, text: &str) -> Utterance {
    Utterance {
        id: id.into(),
        meeting_id: meeting_id.into(),
        source_id: "incoming".into(),
        speaker_id: None,
        start_ms,
        end_ms: start_ms + 1_000,
        status: SegmentStatus::Final,
        language: Language::En,
        revision: 1,
        text: text.into(),
    }
}

fn stored_line_ids(s: &Store, meeting_id: &str) -> Vec<String> {
    let mut stmt = s
        .conn()
        .prepare("SELECT id FROM utterances WHERE meeting_id = ?1 ORDER BY start_ms")
        .expect("prepare");
    stmt.query_map([meeting_id], |r| r.get::<_, String>(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows")
}

fn point(text: &str, ids: &[&str]) -> Point {
    Point {
        text: text.into(),
        line_ids: ids.iter().map(|i| (*i).to_string()).collect(),
    }
}

#[test]
fn migrate_gives_schema_v1_and_a_second_migrate_changes_nothing() {
    let s = ready_store();
    assert_eq!(s.schema_version().expect("version"), store::SCHEMA_VERSION);
    s.migrate().expect("second migrate");
    assert_eq!(s.schema_version().expect("version"), 1);
}

#[test]
fn a_summary_point_with_an_unknown_line_id_is_rejected() {
    let s = ready_store();
    s.insert_meeting("m1", "Weekly sync", 10).expect("meeting");
    s.upsert_utterance(&final_line("u1", "m1", 0, "We ship on Friday."))
        .expect("u1");
    s.upsert_utterance(&final_line("u2", "m1", 2_000, "Ann owns the release."))
        .expect("u2");
    let known = stored_line_ids(&s, "m1");
    assert_eq!(known, ["u1", "u2"]);

    let points = [
        point("Ship on Friday", &["u1"]),
        point("Budget was cut", &["u1", "u99"]),
        point("Ann owns the release", &["u2"]),
    ];
    let report = check_citations(&points, &known);

    assert_eq!(report.accepted.len(), 2);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].point.text, "Budget was cut");
    assert_eq!(
        report.rejected[0].reason,
        RejectReason::UnknownLineIds(vec!["u99".to_string()])
    );
    assert!(!report.is_clean());
}

#[test]
fn a_reply_with_a_bad_point_does_not_replace_existing_notes() {
    let known = vec!["u1".to_string()];
    let existing = [point("Old note", &["u1"])];
    let report = check_citations(&[point("Good", &["u1"]), point("Bad", &["nope"])], &known);
    assert_eq!(decide_notes(&existing, &report), NotesDecision::Keep);

    // With no notes yet, the good points are kept and the bad one is dropped.
    assert_eq!(
        decide_notes(&[], &report),
        NotesDecision::Replace(vec![point("Good", &["u1"])])
    );
}

#[test]
fn deleting_a_meeting_removes_its_rows_and_keeps_the_other_meeting() {
    let s = ready_store();
    s.insert_meeting("m1", "Weekly sync", 10).expect("m1");
    s.insert_meeting("m2", "Other", 20).expect("m2");
    s.upsert_utterance(&final_line("u1", "m1", 0, "one"))
        .expect("u1");
    s.upsert_utterance(&final_line("u2", "m1", 1_000, "two"))
        .expect("u2");
    s.upsert_utterance(&final_line("u3", "m2", 0, "three"))
        .expect("u3");
    s.conn()
        .execute(
            "INSERT INTO summaries (id, meeting_id, body, created_ms) VALUES ('su1', 'm1', 'b', 1)",
            [],
        )
        .expect("summary");
    assert_eq!(count(&s, "utterances"), 3);

    assert!(s.delete_meeting("m1").expect("delete").deleted);

    assert_eq!(stored_line_ids(&s, "m1"), Vec::<String>::new());
    assert_eq!(stored_line_ids(&s, "m2"), ["u3"]);
    assert_eq!(count(&s, "summaries"), 0);
    assert_eq!(count(&s, "meetings"), 1);
    // A second delete finds nothing.
    assert!(!s.delete_meeting("m1").expect("delete again").deleted);
}

#[test]
fn deleting_a_meeting_deletes_its_notes_and_keeps_daily_notes_and_tasks() {
    let s = ready_store();
    s.insert_meeting("m1", "Weekly sync", 10).expect("m1");
    s.insert_meeting("m2", "Other", 20).expect("m2");
    s.insert_note("n1", Some("m1"), "budget from the call", None, 1)
        .expect("n1");
    s.insert_note("n2", Some("m2"), "budget other call", None, 2)
        .expect("n2");
    s.insert_note("daily", None, "budget daily", Some("work"), 3)
        .expect("daily");
    s.conn()
        .execute(
            "INSERT INTO tasks (id, meeting_id, title, status) VALUES ('t1', 'm1', 'Send it', 'open')",
            [],
        )
        .expect("task");
    assert_eq!(s.search_notes("budget").expect("search").len(), 3);

    assert!(s.delete_meeting("m1").expect("delete").deleted);

    let mut found = s.search_notes("budget").expect("search after");
    found.sort();
    assert_eq!(found, ["daily", "n2"]);
    assert_eq!(count(&s, "notes"), 2);
    assert_eq!(count(&s, "tasks"), 1);
    let link: Option<String> = s
        .conn()
        .query_row("SELECT meeting_id FROM tasks WHERE id = 't1'", [], |r| {
            r.get(0)
        })
        .expect("task link");
    assert_eq!(link, None);
}

#[test]
fn a_final_line_cannot_be_rewritten() {
    let s = ready_store();
    s.insert_meeting("m1", "Weekly sync", 10).expect("meeting");
    s.upsert_utterance(&final_line("u1", "m1", 0, "original"))
        .expect("u1");
    let rewrite = final_line("u1", "m1", 0, "changed");
    assert!(matches!(
        s.upsert_utterance(&rewrite),
        Err(store::StoreError::FinalLineImmutable(_))
    ));
}

#[test]
fn notes_export_shows_the_tag_and_the_due_date() {
    // 1_700_000_000_000 ms is 2023-11-14 22:13:20 UTC.
    let created = 1_700_000_000_000;
    let notes = [Note {
        id: "n1".into(),
        text: "Call the vendor".into(),
        tag: Some("work".into()),
        created_ms: created,
    }];
    let tasks = [
        Task {
            id: "t1".into(),
            title: "Send the report".into(),
            due_ms: Some(created + DAY_MS),
            status: TaskStatus::Open,
        },
        Task {
            id: "t2".into(),
            title: "Book a room".into(),
            due_ms: None,
            status: TaskStatus::Done,
        },
    ];

    let md = export_markdown(&notes, &tasks, 0);

    assert_eq!(
        md,
        "# Notes\n\n- 2023-11-14 Call the vendor #work\n\n# Tasks\n\n\
         - [ ] Send the report (due 2023-11-15)\n- [x] Book a room\n"
    );
}
