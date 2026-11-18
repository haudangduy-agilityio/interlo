//! Deleting one meeting removes its text from the store files and the journal
//! files, including the write-ahead logs (ADR 0008).

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use meeting_core::journal::Journal;
use meeting_core::protocol::{Envelope, EventPayload, Language, SegmentStatus, TranscriptSegment};
use std::path::{Path, PathBuf};
use store::{Store, Utterance};

const SECRET: &str = "the quarterly numbers are confidential";
const MEETING_ID: &str = "m1";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("delete-meeting-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn contains(bytes: &[u8]) -> bool {
    bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes())
}

/// Bytes of the database file and its write-ahead log, joined.
fn on_disk(db: &Path) -> Vec<u8> {
    let mut bytes = std::fs::read(db).unwrap_or_default();
    bytes.extend(std::fs::read(format!("{}-wal", db.display())).unwrap_or_default());
    bytes
}

fn journal_event(seq: u64) -> Envelope {
    let payload = EventPayload::TranscriptSegmentUpdated(TranscriptSegment {
        segment_id: format!("seg_{seq}"),
        source_id: "incoming".into(),
        revision: 1,
        start_ms: 0,
        end_ms: 1000,
        status: SegmentStatus::Final,
        language: Language::En,
        text: SECRET.into(),
    });
    Envelope::new(
        format!("e{seq}"),
        MEETING_ID,
        seq,
        "2026-11-13T00:00:00.000Z",
        &payload,
    )
    .expect("envelope")
}

#[test]
fn deleting_a_meeting_scrubs_its_text_from_every_database_and_log_file() {
    let dir = TempDir::new();
    let store_path = dir.0.join("store.db");
    let journal_path = dir.0.join("journal.db");

    let store = Store::open(&store_path).expect("store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(MEETING_ID, "Board call", 0)
        .expect("meeting");
    let journal = Journal::open(&journal_path.to_string_lossy()).expect("journal");
    for seq in 1..=20 {
        store
            .upsert_utterance(&Utterance {
                id: format!("u{seq}"),
                meeting_id: MEETING_ID.into(),
                source_id: "incoming".into(),
                speaker_id: None,
                start_ms: seq * 1_000,
                end_ms: seq * 1_000 + 900,
                status: SegmentStatus::Final,
                language: Language::En,
                revision: 1,
                text: SECRET.into(),
            })
            .expect("utterance");
        journal.append(&journal_event(seq)).expect("append");
    }
    assert!(
        contains(&on_disk(&store_path)),
        "test precondition: the text is in the store files before delete"
    );
    assert!(
        contains(&on_disk(&journal_path)),
        "test precondition: the text is in the journal files before delete"
    );

    // The one user action: both deletes, both connections still open.
    let meeting = store.delete_meeting(MEETING_ID).expect("store delete");
    let session = journal.delete_session(MEETING_ID).expect("journal delete");

    assert!(meeting.deleted && meeting.scrubbed, "{meeting:?}");
    assert!(session.removed == 20 && session.scrubbed, "{session:?}");
    assert!(
        !contains(&on_disk(&store_path)),
        "text left in the store database or its -wal file"
    );
    assert!(
        !contains(&on_disk(&journal_path)),
        "text left in the journal database or its -wal file"
    );
}
