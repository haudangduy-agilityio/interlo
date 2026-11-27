//! Deleting one meeting removes its journal rows and leaves none of its text in
//! the database file or the write-ahead log (ADR 0008).

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use meeting_core::protocol::{Envelope, EventPayload, Language, SegmentStatus, TranscriptSegment};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use store::{MeetingDeletion, Store, Utterance};

const MEETING_ID: &str = "m1";
const KEPT_MEETING_ID: &str = "m3";
/// Seed of the words of `m1`. With this load, words are left on disk when the
/// delete does not rebuild the file.
const SEED: u64 = 0x5EED_CAFE_F00D_1235;
const LINES: u64 = 400;
/// Letters of one random word.
const WORD_LEN: usize = 12;
/// Letters at the end of a word that are searched for. The search index of
/// notes stores a word in parts, so a whole word may not appear in it.
const TAIL_LEN: usize = 8;

type Tail = [u8; TAIL_LEN];

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

/// Seeded random words, with the tail of every word that was handed out.
struct Words {
    state: u64,
    tails: HashSet<Tail>,
}

impl Words {
    fn new(seed: u64) -> Self {
        Self {
            state: seed,
            tails: HashSet::new(),
        }
    }

    /// A number below `limit` (xorshift).
    fn below(&mut self, limit: u64) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state % limit
    }

    fn word(&mut self) -> String {
        let word: String = (0..WORD_LEN)
            .map(|_| char::from(b'a' + self.below(26) as u8))
            .collect();
        let tail = Tail::try_from(&word.as_bytes()[WORD_LEN - TAIL_LEN..]).expect("tail");
        self.tails.insert(tail);
        word
    }

    /// Between 1 and `most` words.
    fn text(&mut self, most: u64) -> String {
        let count = 1 + self.below(most);
        (0..count)
            .map(|_| self.word())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The tails that appear in the database file or its write-ahead log.
fn on_disk(db: &Path, tails: &HashSet<Tail>) -> HashSet<Tail> {
    let log = PathBuf::from(format!("{}-wal", db.display()));
    let mut found = HashSet::new();
    for file in [db.to_path_buf(), log] {
        for window in std::fs::read(file).unwrap_or_default().windows(TAIL_LEN) {
            let window = Tail::try_from(window).expect("window");
            if tails.contains(&window) {
                found.insert(window);
            }
        }
    }
    found
}

fn event(id: &str, session: &str, sequence: u64, text: &str) -> Envelope {
    let payload = EventPayload::TranscriptSegmentUpdated(TranscriptSegment {
        segment_id: format!("seg_{sequence}"),
        source_id: "incoming".into(),
        revision: 1,
        start_ms: 0,
        end_ms: 1000,
        status: SegmentStatus::Final,
        language: Language::En,
        text: text.into(),
    });
    Envelope::new(id, session, sequence, "2026-11-13T00:00:00.000Z", &payload).expect("envelope")
}

fn count(store: &Store, sql: &str) -> i64 {
    store
        .conn()
        .query_row(sql, [], |r| r.get(0))
        .expect("count")
}

/// One final line and one journal event of meeting `m3`, which stays. Its
/// words are not searched for.
fn write_kept_line(store: &Store, kept: &mut Words, number: u64) {
    store
        .upsert_utterance(&Utterance {
            id: kept.word(),
            meeting_id: KEPT_MEETING_ID.into(),
            source_id: "incoming".into(),
            speaker_id: None,
            start_ms: number * 1_000,
            end_ms: number * 1_000 + 900,
            status: SegmentStatus::Final,
            language: Language::En,
            revision: 1,
            text: kept.text(40),
        })
        .expect("kept line");
    store
        .journal()
        .append(&event(
            &kept.word(),
            KEPT_MEETING_ID,
            number,
            &kept.text(40),
        ))
        .expect("kept event");
}

/// Fill meeting `m1` so that every text which its delete removes is made of
/// random words. Lines are revised once, with ids in random order, so that
/// pages split. Lines of meeting `m3` are written in between: pages keep rows
/// after the delete, and old copies of the text of `m1` stay in their free
/// space unless the file is rebuilt.
fn fill_meeting(store: &Store, words: &mut Words) {
    let mut kept = Words::new(7);
    store
        .insert_meeting(KEPT_MEETING_ID, "stays", 0)
        .expect("kept meeting");
    let title = words.text(4);
    store
        .insert_meeting(MEETING_ID, &title, 0)
        .expect("meeting");
    for _ in 0..3 {
        store
            .conn()
            .execute(
                "INSERT INTO speakers (meeting_id, id, label) VALUES (?1, ?2, ?3)",
                [MEETING_ID, &words.word(), &words.text(3)],
            )
            .expect("speaker");
    }
    for number in 1..=LINES {
        let mut line = Utterance {
            id: words.word(),
            meeting_id: MEETING_ID.into(),
            source_id: "incoming".into(),
            speaker_id: None,
            start_ms: number * 1_000,
            end_ms: number * 1_000 + 900,
            status: SegmentStatus::Partial,
            language: Language::En,
            revision: 1,
            text: words.text(20),
        };
        store.upsert_utterance(&line).expect("partial line");
        line.status = SegmentStatus::Final;
        line.revision = 2;
        line.text = words.text(40);
        store.upsert_utterance(&line).expect("final line");
        write_kept_line(store, &mut kept, number);
        let id = words.word();
        let spoken = words.text(40);
        store
            .journal()
            .append(&event(&id, MEETING_ID, number, &spoken))
            .expect("event");
    }
    for _ in 0..3 {
        store
            .conn()
            .execute(
                "INSERT INTO summaries (id, meeting_id, body, created_ms) VALUES (?1, ?2, ?3, 0)",
                [&words.word(), MEETING_ID, &words.text(60)],
            )
            .expect("summary");
        store
            .conn()
            .execute(
                "INSERT INTO insights (id, meeting_id, kind, body) VALUES (?1, ?2, ?3, ?4)",
                [&words.word(), MEETING_ID, &words.word(), &words.text(30)],
            )
            .expect("insight");
        store
            .conn()
            .execute(
                "INSERT INTO jobs (id, meeting_id, kind, state, run_at_ms, payload)
                 VALUES (?1, ?2, 'summarize', 'queued', 0, ?3)",
                [&words.word(), MEETING_ID, &words.text(30)],
            )
            .expect("job");
    }
    for _ in 0..2 {
        let id = words.word();
        let text = words.text(30);
        store
            .insert_note(&id, Some(MEETING_ID), &text, None, 1)
            .expect("note");
    }
}

#[test]
fn deleting_a_meeting_scrubs_its_text_from_the_database_and_log() {
    let dir = TempDir::new();
    let path = dir.0.join("interlo.db");
    let store = Store::open(&path).expect("store");
    let mut words = Words::new(SEED);
    fill_meeting(&store, &mut words);
    store.insert_meeting("m2", "other", 0).expect("meeting");
    store
        .journal()
        .append(&event("kept", "m2", 1, "kept line"))
        .expect("event");
    assert!(
        on_disk(&path, &words.tails).len() > words.tails.len() / 2,
        "test precondition: the words are on disk before the delete"
    );

    // The one user action, with the connection still open.
    let outcome = store.delete_meeting(MEETING_ID).expect("delete");

    assert_eq!(
        outcome,
        MeetingDeletion {
            deleted: true,
            scrubbed: true
        }
    );
    assert!(
        store.journal().load(MEETING_ID).expect("load").is_empty(),
        "journal rows left"
    );
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM utterances WHERE meeting_id = 'm1'"
        ),
        0
    );
    assert_eq!(store.journal().load("m2").expect("load").len(), 1);
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM utterances WHERE meeting_id = 'm3'"
        ),
        LINES as i64,
        "lines of the meeting that stays"
    );
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM settings WHERE key = 'store.cleanup_pending'"
        ),
        0,
        "no cleanup is pending"
    );
    let left = on_disk(&path, &words.tails);
    assert!(
        left.is_empty(),
        "{} of {} words are left on disk",
        left.len(),
        words.tails.len()
    );
}
