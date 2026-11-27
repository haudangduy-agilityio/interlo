//! Replaying a recorded session gives the same captions every time.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use meeting_core::coordinator::{Applied, Caption, SessionView};
use meeting_core::protocol::{Envelope, SegmentStatus, SessionStatus, TranslationStatus};
use store::{AppendOutcome, Store};

const SESSION_ID: &str = "session_01";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/protocol/session_example.jsonl")
}

fn load_fixture() -> Vec<Envelope> {
    let text = fs::read_to_string(fixture_path()).expect("fixture is readable");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("fixture line is an envelope"))
        .collect()
}

fn replay(events: &[Envelope]) -> (Vec<Caption>, SessionStatus) {
    let mut view = SessionView::new(SESSION_ID);
    for env in events {
        assert_eq!(view.apply(env), Applied::Applied, "event {}", env.event_id);
    }
    (view.captions(), view.status())
}

#[test]
fn replaying_the_fixture_twice_gives_the_same_captions() {
    let events = load_fixture();
    assert_eq!(events.len(), 9);

    let first = replay(&events);
    let second = replay(&events);
    assert_eq!(first, second);
}

#[test]
fn replayed_captions_show_the_final_line_with_its_matching_translation() {
    let (captions, status) = replay(&load_fixture());
    assert_eq!(status, SessionStatus::Completed);
    assert_eq!(captions.len(), 1);
    let c = &captions[0];
    assert_eq!(c.segment_id, "seg_1");
    assert_eq!(c.source_text, "We can ship this on Friday.");
    assert_eq!(c.source_status, SegmentStatus::Final);
    assert_eq!(c.translation_status, Some(TranslationStatus::Final));
    assert_eq!(
        c.target_text.as_deref(),
        Some("Chúng ta có thể phát hành vào thứ Sáu.")
    );
}

#[test]
fn a_journal_round_trip_replays_to_the_same_captions_as_the_file() {
    let events = load_fixture();
    let store = Store::open_in_memory().expect("in-memory store");
    let journal = store.journal();
    for env in &events {
        assert_eq!(
            journal.append(env).expect("append"),
            AppendOutcome::Inserted
        );
    }
    // Appending the same events again is a no-op, so a replay cannot double up.
    for env in &events {
        assert_eq!(
            journal.append(env).expect("append"),
            AppendOutcome::Duplicate
        );
    }

    let loaded = journal.load(SESSION_ID).expect("load");
    assert_eq!(loaded, events);
    assert_eq!(replay(&loaded), replay(&events));
}

#[test]
fn events_replay_in_sequence_order_per_session() {
    let store = Store::open_in_memory().expect("in-memory store");
    let journal = store.journal();
    let events = load_fixture();
    let mut other = events[0].clone();
    other.event_id = "evt_other".into();
    other.session_id = "session_02".into();
    // Append out of order on purpose; the journal must replay by sequence.
    for env in events.iter().rev().chain([&other]) {
        assert_eq!(
            journal.append(env).expect("append"),
            AppendOutcome::Inserted
        );
    }
    assert_eq!(
        journal.append(&events[0]).expect("append"),
        AppendOutcome::Duplicate
    );

    let loaded = journal.load(SESSION_ID).expect("load");
    assert_eq!(loaded, events);

    let (_, status) = replay(&loaded);
    assert_eq!(status, SessionStatus::Completed);
    assert!(journal.load("session_unknown").expect("load").is_empty());
}

#[test]
fn feeding_the_same_events_again_is_a_no_op() {
    let events = load_fixture();
    let mut view = SessionView::new(SESSION_ID);
    for env in &events {
        view.apply(env);
    }
    let once = view.captions();
    for env in &events {
        assert_eq!(view.apply(env), Applied::Duplicate);
    }
    assert_eq!(view.captions(), once);
}
