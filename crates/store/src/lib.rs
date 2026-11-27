//! store: SQLite projections of meetings, notes, tasks, jobs, models and settings.
//!
//! Owns: the one database file, its single schema version and every migration,
//! WAL mode, foreign keys and the full-text index over notes. Everything is
//! stored as text and integers.
//!
//! Must never: store audio, or rewrite a final line. A `final` utterance is
//! immutable: `Store::upsert_utterance` refuses to touch it, and triggers abort
//! a raw `UPDATE`, an `INSERT OR REPLACE` over it, a change of a line id and a
//! raw `DELETE` of it. Journal rows have the same protection. The only delete
//! that may remove a final line or a journal row is `Store::delete_meeting`.

use meeting_core::protocol::{Language, SegmentStatus};
use rusqlite::config::DbConfig;
use rusqlite::functions::FunctionFlags;
use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

mod files;
mod journal;
mod schema;

pub use journal::{AppendOutcome, Journal, JournalError};
pub use schema::SCHEMA_VERSION;
use schema::{Found, Migration, MIGRATIONS};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("final utterance {0} cannot be rewritten")]
    FinalLineImmutable(String),
    #[error("utterance {id} has revision {stored}; revision {offered} is not newer")]
    StaleRevision {
        id: String,
        stored: u32,
        offered: u32,
    },
    #[error("database schema v{found} has a change that this build (v{supported}) cannot use")]
    IncompatibleSchema { found: i64, supported: i64 },
    #[error("database schema v{found} is not one that this app wrote")]
    UnknownSchema { found: i64 },
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("time {0} ms does not fit a database integer")]
    TimeOutOfRange(u64),
}

/// One stored transcript line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utterance {
    pub id: String,
    pub meeting_id: String,
    pub source_id: String,
    pub speaker_id: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub status: SegmentStatus,
    pub language: Language,
    pub revision: u32,
    pub text: String,
}

fn status_str(s: SegmentStatus) -> &'static str {
    match s {
        SegmentStatus::Partial => "partial",
        SegmentStatus::Stable => "stable",
        SegmentStatus::Final => "final",
    }
}

fn language_str(l: Language) -> &'static str {
    match l {
        Language::En => "en",
        Language::Vi => "vi",
    }
}

/// Turn free user text into a safe FTS5 query: every whitespace-separated
/// token becomes a quoted phrase, so operators and punctuation are literal.
pub fn fts_query(user_text: &str) -> String {
    user_text
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Default number of ids that `search_notes` returns.
pub const DEFAULT_SEARCH_LIMIT: usize = 50;

/// Wait this long for a lock before a statement fails with `SQLITE_BUSY`.
const BUSY_TIMEOUT_MS: u64 = 5_000;
/// Pause between two tries to change the journal mode. Starting value.
const WAL_RETRY_PAUSE_MS: u64 = 10;

/// Switch to write-ahead logging. SQLite does not apply the busy timeout to
/// this change, so a busy file is tried again for as long as that timeout.
fn switch_to_wal(conn: &Connection) -> rusqlite::Result<()> {
    let give_up = Instant::now() + Duration::from_millis(BUSY_TIMEOUT_MS);
    loop {
        match conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0)) {
            Err(e)
                if e.sqlite_error_code() == Some(ErrorCode::DatabaseBusy)
                    && Instant::now() < give_up =>
            {
                std::thread::sleep(Duration::from_millis(WAL_RETRY_PAUSE_MS));
            }
            other => return other.map(|_| ()),
        }
    }
}

/// UTC milliseconds of the system clock. A clock before 1970 gives 0.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(0)
}

/// SQLite integers are signed 64-bit. A time in milliseconds above that range
/// cannot be stored, so it is refused instead of wrapped.
fn ms_to_sql(ms: u64) -> Result<i64, StoreError> {
    i64::try_from(ms).map_err(|_| StoreError::TimeOutOfRange(ms))
}

/// The settings row that a meeting delete writes in its own transaction and
/// that the cleanup removes.
const CLEANUP_PENDING_KEY: &str = "store.cleanup_pending";

/// Rebuilds the database file, so that no free page keeps old text.
pub(crate) type Rebuild = fn(&Connection) -> rusqlite::Result<()>;

pub(crate) fn vacuum(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("VACUUM")
}

/// What `Store::delete_meeting` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeetingDeletion {
    /// True when a meeting with that id existed and was deleted.
    pub deleted: bool,
    /// True when the database file was rebuilt and the write-ahead log
    /// truncated, so neither keeps a copy of the deleted text (ADR 0008).
    /// False means the cleanup is pending: `Store::scrub` and the next open try
    /// it again.
    pub scrubbed: bool,
}

pub struct Store {
    conn: Connection,
    /// Read by the SQL function `store_delete_open()`. True only while a
    /// meeting delete runs.
    cascade_open: Arc<AtomicBool>,
}

/// Opens the delete guard of final lines and journal rows and closes it again
/// on every exit path.
struct CascadeScope<'a>(&'a AtomicBool);

impl<'a> CascadeScope<'a> {
    fn open(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(flag)
    }
}

impl Drop for CascadeScope<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Store {
    /// Open (or create) the database file in WAL mode and bring it to the
    /// schema of this build. A file that this build must not use is refused with
    /// `IncompatibleSchema` or `UnknownSchema` and keeps its content. The
    /// directory (mode 0700 when created here) and the files (mode 0600) are
    /// made owner-only before SQLite opens them.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_with(path.as_ref(), MIGRATIONS, now_ms(), vacuum)
    }

    /// `open` with the migration list, the migration time and the rebuild step
    /// of the cleanup passed in.
    pub(crate) fn open_with(
        path: &Path,
        migrations: &[Migration],
        now_ms: i64,
        rebuild: Rebuild,
    ) -> Result<Self, StoreError> {
        if files::is_empty_with_log(path)? {
            files::tighten_existing(path)?;
            return Err(StoreError::UnknownSchema { found: 0 });
        }
        files::prepare_private_db(path)?;
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
        // A refused database keeps its bytes: until the open is accepted,
        // closing the connection must not move the log into the file.
        conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
        let found = {
            // One read transaction: version and records belong to one state.
            let read = conn.unchecked_transaction()?;
            schema::inspect(&read, migrations)?
        };
        switch_to_wal(&conn)?;
        let store = Self::init(conn)?;
        if let Found::Behind(_) = found {
            schema::apply_pending(&store.conn, migrations, now_ms)?;
        }
        store
            .conn
            .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, false)?;
        store.finish_open(rebuild);
        files::tighten_sidecars(path)?;
        Ok(store)
    }

    /// In-memory database for tests. WAL does not apply to memory databases.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let store = Self::init(Connection::open_in_memory()?)?;
        schema::apply_pending(&store.conn, MIGRATIONS, now_ms())?;
        store.finish_open(vacuum);
        Ok(store)
    }

    /// The last step of an accepted open: run a pending cleanup, else empty the
    /// log. Neither step fails the open; a cleanup that cannot finish stays
    /// pending.
    fn finish_open(&self, rebuild: Rebuild) {
        if matches!(self.cleanup_pending(), Ok(true)) {
            let _ = self.clean_up(rebuild);
        } else {
            let _ = files::truncate_wal(&self.conn);
        }
    }

    /// Per-connection setup, shared by every open path: foreign keys on,
    /// freed pages overwritten with zeros (deleted text must not stay in the
    /// file), NORMAL sync (safe in WAL mode), a 5 s lock wait and the delete
    /// guard function.
    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "secure_delete", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let cascade_open = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cascade_open);
        conn.create_scalar_function(
            "store_delete_open",
            0,
            FunctionFlags::SQLITE_UTF8,
            move |_| Ok(i64::from(flag.load(Ordering::SeqCst))),
        )?;
        Ok(Self { conn, cascade_open })
    }

    /// Raw connection for read queries and tests. Final utterances stay
    /// protected by triggers even through this handle, but a caller that drops
    /// a trigger or the schema can still break the rule: do not write schema
    /// from here.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The append-only event journal, on the connection of the store.
    pub fn journal(&self) -> Journal<'_> {
        Journal::new(&self.conn)
    }

    pub fn schema_version(&self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    pub fn insert_meeting(&self, id: &str, title: &str, started_ms: u64) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO meetings (id, title, started_ms) VALUES (?1, ?2, ?3)",
            params![id, title, ms_to_sql(started_ms)?],
        )?;
        Ok(())
    }

    /// Delete a meeting in one transaction (D-46). Foreign keys cascade to its
    /// utterances (final lines included: this is the only path allowed to
    /// delete them), speakers, summaries and insights. Its journal rows and its
    /// jobs have no foreign key, so they are deleted explicitly; this is the
    /// only path allowed to delete journal rows. So are the notes created in the
    /// meeting (`notes.meeting_id`); the `notes_fts_delete` trigger removes
    /// their search rows. Daily notes (`meeting_id` NULL) and other meetings'
    /// notes stay.
    ///
    /// Tasks stay: `ON DELETE SET NULL` clears the link, because a confirmed
    /// task is the owner's to-do item. Unconfirmed task drafts are not stored
    /// (they live in `workspace-notes` memory only), so there is nothing to
    /// delete for them here.
    ///
    /// After the commit the file is rebuilt and the write-ahead log truncated
    /// (ADR 0008), so the deleted text stays in neither. If that cannot finish,
    /// `scrubbed` is false and the cleanup stays pending. The rebuild takes time
    /// in proportion to the size of the file.
    pub fn delete_meeting(&self, id: &str) -> Result<MeetingDeletion, StoreError> {
        self.delete_meeting_with(id, vacuum)
    }

    /// `delete_meeting` with the rebuild step of the cleanup passed in.
    fn delete_meeting_with(
        &self,
        id: &str,
        rebuild: Rebuild,
    ) -> Result<MeetingDeletion, StoreError> {
        let deleted = self.commit_meeting_delete(id)?;
        // The rows are gone. A cleanup that fails is reported, not an error.
        let scrubbed = self.clean_up(rebuild).unwrap_or(false);
        Ok(MeetingDeletion { deleted, scrubbed })
    }

    /// The transaction of a meeting delete, without the cleanup. Returns true
    /// when a meeting row was deleted.
    fn commit_meeting_delete(&self, id: &str) -> Result<bool, StoreError> {
        let _open = CascadeScope::open(&self.cascade_open);
        let tx = self.conn.unchecked_transaction()?;
        // The session id of a journal row is the meeting id.
        tx.execute("DELETE FROM events WHERE session_id = ?1", params![id])?;
        tx.execute("DELETE FROM jobs WHERE meeting_id = ?1", params![id])?;
        tx.execute("DELETE FROM notes WHERE meeting_id = ?1", params![id])?;
        let n = tx.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        // In the same transaction: a committed delete stays marked until the
        // file is clean, also when the app exits before the cleanup.
        tx.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, '1')",
            params![CLEANUP_PENDING_KEY],
        )?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Rebuild the file and truncate the log again, to finish the cleanup of a
    /// meeting delete. `Ok(true)` when both worked.
    pub fn scrub(&self) -> Result<bool, StoreError> {
        self.clean_up(vacuum)
    }

    fn cleanup_pending(&self) -> rusqlite::Result<bool> {
        self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM settings WHERE key = ?1)",
            params![CLEANUP_PENDING_KEY],
            |r| r.get(0),
        )
    }

    /// Rebuild the file, truncate the log, and remove the pending mark when
    /// both worked.
    fn clean_up(&self, rebuild: Rebuild) -> Result<bool, StoreError> {
        let rebuilt = rebuild(&self.conn);
        // The log holds the deleted text too: truncate it even when the rebuild
        // failed.
        let truncated = files::truncate_wal(&self.conn)?;
        rebuilt?;
        if truncated {
            // The file is clean. A mark that cannot be removed only repeats the
            // cleanup at the next open.
            let _ = self.conn.execute(
                "DELETE FROM settings WHERE key = ?1",
                params![CLEANUP_PENDING_KEY],
            );
        }
        Ok(truncated)
    }

    /// Insert a line, or revise it while it is not final. A final line is never
    /// rewritten: the call fails with `FinalLineImmutable`. A revision that is
    /// not higher than the stored one never overwrites it: the call fails with
    /// `StaleRevision` and the stored line stays.
    pub fn upsert_utterance(&self, u: &Utterance) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        let stored: Option<(String, u32)> = tx
            .query_row(
                "SELECT status, revision FROM utterances WHERE id = ?1",
                params![u.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match stored {
            Some((status, _)) if status == "final" => {
                return Err(StoreError::FinalLineImmutable(u.id.clone()));
            }
            Some((_, revision)) if u.revision <= revision => {
                return Err(StoreError::StaleRevision {
                    id: u.id.clone(),
                    stored: revision,
                    offered: u.revision,
                });
            }
            _ => {}
        }
        tx.execute(
            "INSERT INTO utterances
                 (id, meeting_id, source_id, speaker_id, start_ms, end_ms, status, language, revision, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                 speaker_id = excluded.speaker_id,
                 end_ms     = excluded.end_ms,
                 status     = excluded.status,
                 revision   = excluded.revision,
                 text       = excluded.text
             WHERE utterances.status <> 'final' AND excluded.revision > utterances.revision",
            params![
                u.id,
                u.meeting_id,
                u.source_id,
                u.speaker_id,
                ms_to_sql(u.start_ms)?,
                ms_to_sql(u.end_ms)?,
                status_str(u.status),
                language_str(u.language),
                u.revision,
                u.text
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Add a note. The tag row is created on first use.
    pub fn insert_note(
        &self,
        id: &str,
        meeting_id: Option<&str>,
        text: &str,
        tag: Option<&str>,
        created_ms: u64,
    ) -> Result<(), StoreError> {
        if let Some(t) = tag {
            self.conn
                .execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", params![t])?;
        }
        self.conn.execute(
            "INSERT INTO notes (id, meeting_id, text, tag, created_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, meeting_id, text, tag, ms_to_sql(created_ms)?],
        )?;
        Ok(())
    }

    /// Ids of notes that match the user's text, best match first, at most
    /// `DEFAULT_SEARCH_LIMIT` of them.
    pub fn search_notes(&self, user_text: &str) -> Result<Vec<String>, StoreError> {
        self.search_notes_with_limit(user_text, DEFAULT_SEARCH_LIMIT)
    }

    /// Like `search_notes`, with the caller's limit.
    pub fn search_notes_with_limit(
        &self,
        user_text: &str,
        limit: usize,
    ) -> Result<Vec<String>, StoreError> {
        let q = fts_query(user_text);
        if q.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut stmt = self.conn.prepare(
            "SELECT n.id FROM notes_fts
             JOIN notes n ON n.seq = notes_fts.rowid
             WHERE notes_fts MATCH ?1
             ORDER BY notes_fts.rank
             LIMIT ?2",
        )?;
        let ids = stmt
            .query_map(params![q, limit], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Retention sweep. Stub: the policy is decided in a later task.
    pub fn purge_meetings_before(&self, _cutoff_ms: u64) -> Result<usize, StoreError> {
        Err(StoreError::NotImplemented("purge_meetings_before"))
    }
}

/// Helpers that the tests of this crate share.
#[cfg(test)]
pub(crate) mod testing {
    use meeting_core::protocol::{
        Envelope, EventPayload, Language, SegmentStatus, TranscriptSegment,
    };
    use rusqlite::config::DbConfig;
    use rusqlite::Connection;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A new folder that is removed with the value.
    pub(crate) struct TempDir(PathBuf);

    impl TempDir {
        pub(crate) fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "store-test-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }

        pub(crate) fn db(&self) -> PathBuf {
            self.0.join("store.db")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A journal event of a final transcript line.
    pub(crate) fn event(id: &str, session: &str, sequence: u64, text: &str) -> Envelope {
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
        Envelope::new(id, session, sequence, "2026-11-26T00:00:00.000Z", &payload).unwrap()
    }

    pub(crate) fn wal_of(db: &Path) -> PathBuf {
        PathBuf::from(format!("{}-wal", db.display()))
    }

    pub(crate) fn bytes(path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }

    /// A connection without the settings and functions of the store.
    pub(crate) fn plain(db: &Path) -> Connection {
        Connection::open(db).unwrap()
    }

    /// A plain connection that leaves its changes in the write-ahead log when
    /// it closes, as an exit without a checkpoint does.
    pub(crate) fn leaves_its_log(db: &Path) -> Connection {
        let conn = plain(db);
        conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
            .unwrap();
        conn
    }

    pub(crate) fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    pub(crate) fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    /// Every schema object with its definition.
    pub(crate) fn schema_of(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT type || ' ' || name || ' ' || coalesce(sql, '') FROM sqlite_master
                 ORDER BY type, name",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    }

    /// The migration records as (version, name, time, incompatible).
    pub(crate) fn records(conn: &Connection) -> Vec<(i64, String, i64, i64)> {
        let mut stmt = conn
            .prepare(
                "SELECT version, name, applied_ms, incompatible FROM schema_migrations
                 ORDER BY version",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    }

    #[cfg(unix)]
    pub(crate) fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{bytes, event, leaves_its_log, plain, wal_of, TempDir};

    fn ready() -> Store {
        Store::open_in_memory().unwrap()
    }

    fn count(s: &Store, table: &str) -> i64 {
        s.conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn utt(id: &str, status: SegmentStatus, text: &str) -> Utterance {
        Utterance {
            id: id.into(),
            meeting_id: "m1".into(),
            source_id: "incoming".into(),
            speaker_id: Some("s1".into()),
            start_ms: 0,
            end_ms: 1000,
            status,
            language: Language::En,
            revision: 1,
            text: text.into(),
        }
    }

    #[test]
    fn foreign_keys_are_on() {
        let s = ready();
        let on: i64 = s
            .conn()
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(on, 1);
    }

    #[test]
    fn deleting_a_meeting_removes_its_rows() {
        let s = ready();
        s.insert_meeting("m1", "Weekly sync", 10).unwrap();
        s.insert_meeting("m2", "Other", 20).unwrap();
        s.upsert_utterance(&utt("u1", SegmentStatus::Final, "hello"))
            .unwrap();
        s.conn()
            .execute(
                "INSERT INTO speakers (meeting_id, id, label) VALUES ('m1', 's1', 'Ann')",
                [],
            )
            .unwrap();
        s.conn()
            .execute(
                "INSERT INTO summaries (id, meeting_id, body, created_ms) VALUES ('su1', 'm1', 'b', 1)",
                [],
            )
            .unwrap();
        s.conn()
            .execute(
                "INSERT INTO insights (id, meeting_id, kind, body) VALUES ('i1', 'm1', 'risk', 'b')",
                [],
            )
            .unwrap();
        s.conn()
            .execute(
                "INSERT INTO speakers (meeting_id, id, label) VALUES ('m2', 's9', 'Bob')",
                [],
            )
            .unwrap();

        assert!(s.delete_meeting("m1").unwrap().deleted);

        for t in ["utterances", "summaries", "insights"] {
            assert_eq!(count(&s, t), 0, "{t} not cascaded");
        }
        // Only the other meeting's speaker is left.
        assert_eq!(count(&s, "speakers"), 1);
        assert_eq!(count(&s, "meetings"), 1);
    }

    #[test]
    fn utterance_needs_an_existing_meeting() {
        let s = ready();
        assert!(s
            .upsert_utterance(&utt("u1", SegmentStatus::Partial, "x"))
            .is_err());
    }

    #[test]
    fn partial_line_can_be_revised() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.upsert_utterance(&utt("u1", SegmentStatus::Partial, "hel"))
            .unwrap();
        let mut next = utt("u1", SegmentStatus::Stable, "hello");
        next.revision = 2;
        s.upsert_utterance(&next).unwrap();
        let text: String = s
            .conn()
            .query_row("SELECT text FROM utterances WHERE id = 'u1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn final_line_is_never_rewritten() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.upsert_utterance(&utt("u1", SegmentStatus::Final, "hello"))
            .unwrap();

        let err = s
            .upsert_utterance(&utt("u1", SegmentStatus::Final, "changed"))
            .unwrap_err();
        assert!(matches!(err, StoreError::FinalLineImmutable(id) if id == "u1"));

        // A raw UPDATE is stopped by the trigger as well.
        assert!(s
            .conn()
            .execute("UPDATE utterances SET text = 'x' WHERE id = 'u1'", [])
            .is_err());

        let text: String = s
            .conn()
            .query_row("SELECT text FROM utterances WHERE id = 'u1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn no_blob_columns_outside_the_fts_shadow_tables() {
        let s = ready();
        let mut stmt = s
            .conn()
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'notes_fts%'",
            )
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!names.is_empty());
        for n in names {
            let mut info = s
                .conn()
                .prepare(&format!("PRAGMA table_info({n})"))
                .unwrap();
            let types: Vec<String> = info
                .query_map([], |r| r.get::<_, String>(2))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            for t in types {
                assert!(!t.to_uppercase().contains("BLOB"), "{n} has a BLOB column");
            }
        }
    }

    #[test]
    fn note_search_follows_insert_update_delete() {
        let s = ready();
        s.insert_note("n1", None, "buy oat milk", Some("home"), 1)
            .unwrap();
        s.insert_note("n2", None, "call the dentist", None, 2)
            .unwrap();
        assert_eq!(s.search_notes("milk").unwrap(), vec!["n1".to_string()]);

        s.conn()
            .execute(
                "UPDATE notes SET text = 'buy soy drink' WHERE id = 'n1'",
                [],
            )
            .unwrap();
        assert!(s.search_notes("milk").unwrap().is_empty());
        assert_eq!(s.search_notes("soy").unwrap(), vec!["n1".to_string()]);

        s.conn()
            .execute("DELETE FROM notes WHERE id = 'n1'", [])
            .unwrap();
        assert!(s.search_notes("soy").unwrap().is_empty());
    }

    #[test]
    fn search_treats_operators_as_plain_text() {
        let s = ready();
        s.insert_note("n1", None, "alpha beta", None, 1).unwrap();
        assert!(s.search_notes("\"unbalanced AND (").is_ok());
        assert!(s.search_notes("   ").unwrap().is_empty());
        assert_eq!(fts_query("a \"b\""), "\"a\" \"\"\"b\"\"\"");
    }

    fn fts_rows(s: &Store) -> i64 {
        count(s, "notes_fts")
    }

    #[test]
    fn deleting_a_meeting_deletes_its_notes_and_their_search_rows() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.insert_note("n1", Some("m1"), "alpha from the call", None, 1)
            .unwrap();
        s.insert_note("n2", Some("m1"), "beta from the call", Some("work"), 2)
            .unwrap();
        assert_eq!(s.search_notes("alpha").unwrap(), ["n1"]);
        assert_eq!(fts_rows(&s), 2);

        assert!(s.delete_meeting("m1").unwrap().deleted);

        assert_eq!(count(&s, "notes"), 0);
        assert_eq!(fts_rows(&s), 0);
        assert!(s.search_notes("alpha").unwrap().is_empty());
        assert!(s.search_notes("beta").unwrap().is_empty());
    }

    #[test]
    fn deleting_a_meeting_keeps_daily_notes_and_other_meetings_notes() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.insert_meeting("m2", "t", 0).unwrap();
        s.insert_note("n1", Some("m1"), "alpha gone", None, 1)
            .unwrap();
        s.insert_note("daily", None, "alpha daily", None, 2)
            .unwrap();
        s.insert_note("n2", Some("m2"), "alpha other", None, 3)
            .unwrap();

        s.delete_meeting("m1").unwrap();

        let mut found = s.search_notes("alpha").unwrap();
        found.sort();
        assert_eq!(found, ["daily", "n2"]);
        assert_eq!(count(&s, "notes"), 2);
        assert_eq!(fts_rows(&s), 2);
        let link: Option<String> = s
            .conn()
            .query_row("SELECT meeting_id FROM notes WHERE id = 'n2'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(link.as_deref(), Some("m2"));
        let daily: Option<String> = s
            .conn()
            .query_row("SELECT meeting_id FROM notes WHERE id = 'daily'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(daily, None);
    }

    #[test]
    fn deleting_an_unknown_meeting_deletes_no_notes() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.insert_note("n1", Some("m1"), "alpha", None, 1).unwrap();
        s.insert_note("daily", None, "alpha daily", None, 2)
            .unwrap();
        assert!(!s.delete_meeting("nope").unwrap().deleted);
        assert_eq!(count(&s, "notes"), 2);
        assert_eq!(fts_rows(&s), 2);
    }

    #[test]
    fn deleting_a_meeting_keeps_the_task_and_clears_its_link_while_notes_go() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.insert_note("n1", Some("m1"), "alpha", None, 1).unwrap();
        s.conn()
            .execute(
                "INSERT INTO tasks (id, meeting_id, title, status) VALUES ('t1', 'm1', 'x', 'open')",
                [],
            )
            .unwrap();
        s.delete_meeting("m1").unwrap();
        assert_eq!(count(&s, "tasks"), 1);
        assert_eq!(count(&s, "notes"), 0);
    }

    #[test]
    fn file_database_uses_wal() {
        let path = temp_db_path("wal");
        {
            let s = Store::open(&path).unwrap();
            let mode: String = s
                .conn()
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
        }
        remove_db(&path);
    }

    fn temp_db_path(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "store-{tag}-{}-{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ))
    }

    fn remove_db(path: &std::path::Path) {
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    fn raw_utterance_sql(verb: &str, id: &str, status: &str, revision: i64, text: &str) -> String {
        format!(
            "{verb} INTO utterances (id, meeting_id, source_id, start_ms, end_ms, status, \
             language, revision, text) VALUES ('{id}', 'm1', 'incoming', 0, 1000, '{status}', \
             'en', {revision}, '{text}')"
        )
    }

    fn text_of(s: &Store, id: &str) -> String {
        s.conn()
            .query_row("SELECT text FROM utterances WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    fn final_store() -> Store {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.upsert_utterance(&utt("u1", SegmentStatus::Final, "hello"))
            .unwrap();
        s
    }

    #[test]
    fn final_line_survives_insert_or_replace() {
        let s = final_store();
        let sql = raw_utterance_sql("INSERT OR REPLACE", "u1", "partial", 9, "rewritten");
        assert!(s.conn().execute(&sql, []).is_err());
        assert_eq!(text_of(&s, "u1"), "hello");
        assert_eq!(count(&s, "utterances"), 1);
    }

    #[test]
    fn final_line_survives_replace_into_with_a_final_status() {
        let s = final_store();
        let sql = raw_utterance_sql("REPLACE", "u1", "final", 9, "rewritten");
        assert!(s.conn().execute(&sql, []).is_err());
        assert_eq!(text_of(&s, "u1"), "hello");
    }

    #[test]
    fn final_line_survives_a_raw_delete() {
        let s = final_store();
        assert!(s
            .conn()
            .execute("DELETE FROM utterances WHERE id = 'u1'", [])
            .is_err());
        assert!(s.conn().execute("DELETE FROM utterances", []).is_err());
        assert_eq!(count(&s, "utterances"), 1);
    }

    #[test]
    fn final_line_survives_a_raw_meeting_delete() {
        let s = final_store();
        assert!(s
            .conn()
            .execute("DELETE FROM meetings WHERE id = 'm1'", [])
            .is_err());
        assert_eq!(count(&s, "utterances"), 1);
        assert_eq!(count(&s, "meetings"), 1);
    }

    #[test]
    fn delete_meeting_still_removes_final_lines() {
        let s = final_store();
        assert!(s.delete_meeting("m1").unwrap().deleted);
        assert_eq!(count(&s, "utterances"), 0);
        // The guard closes again after the call: a new final line is protected.
        s.insert_meeting("m1", "again", 0).unwrap();
        s.upsert_utterance(&utt("u2", SegmentStatus::Final, "kept"))
            .unwrap();
        assert!(s
            .conn()
            .execute("DELETE FROM utterances WHERE id = 'u2'", [])
            .is_err());
    }

    #[test]
    fn a_failed_meeting_delete_closes_the_guard_again() {
        let s = final_store();
        // Break the delete from inside: the transaction fails on a missing table.
        s.conn().execute_batch("DROP TABLE jobs").unwrap();
        assert!(s.delete_meeting("m1").is_err());
        assert!(s
            .conn()
            .execute("DELETE FROM utterances WHERE id = 'u1'", [])
            .is_err());
        assert_eq!(count(&s, "utterances"), 1);
    }

    #[test]
    fn non_final_lines_can_still_be_deleted_and_replaced() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.upsert_utterance(&utt("u1", SegmentStatus::Partial, "hel"))
            .unwrap();
        let sql = raw_utterance_sql("INSERT OR REPLACE", "u1", "stable", 3, "hello");
        s.conn().execute(&sql, []).unwrap();
        assert_eq!(text_of(&s, "u1"), "hello");
        s.conn()
            .execute("DELETE FROM utterances WHERE id = 'u1'", [])
            .unwrap();
        assert_eq!(count(&s, "utterances"), 0);
    }

    #[test]
    fn lower_revision_does_not_overwrite_a_higher_one() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        let mut high = utt("u1", SegmentStatus::Partial, "new");
        high.revision = 5;
        s.upsert_utterance(&high).unwrap();

        let mut low = utt("u1", SegmentStatus::Stable, "old");
        low.revision = 3;
        let res = s.upsert_utterance(&low);
        assert_eq!(text_of(&s, "u1"), "new");
        assert!(matches!(
            res,
            Err(StoreError::StaleRevision {
                stored: 5,
                offered: 3,
                ..
            })
        ));

        // The same revision is refused too: only a higher one wins.
        let mut same = utt("u1", SegmentStatus::Stable, "same");
        same.revision = 5;
        let res = s.upsert_utterance(&same);
        assert_eq!(text_of(&s, "u1"), "new");
        assert!(matches!(res, Err(StoreError::StaleRevision { .. })));

        let mut higher = utt("u1", SegmentStatus::Stable, "newer");
        higher.revision = 6;
        s.upsert_utterance(&higher).unwrap();
        assert_eq!(text_of(&s, "u1"), "newer");
    }

    fn add_job(s: &Store, id: &str, meeting_id: &str) {
        s.conn()
            .execute(
                "INSERT INTO jobs (id, meeting_id, kind, state, run_at_ms) \
                 VALUES (?1, ?2, 'summarize', 'queued', 0)",
                params![id, meeting_id],
            )
            .unwrap();
    }

    #[test]
    fn deleting_a_meeting_removes_its_jobs_only() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.insert_meeting("m2", "t", 0).unwrap();
        add_job(&s, "j1", "m1");
        add_job(&s, "j2", "m1");
        add_job(&s, "j3", "m2");
        assert!(s.delete_meeting("m1").unwrap().deleted);
        let left: Vec<String> = s
            .conn()
            .prepare("SELECT id FROM jobs")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(left, vec!["j3".to_string()]);
    }

    #[test]
    fn deleting_a_meeting_keeps_tasks_but_detaches_them() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.conn()
            .execute(
                "INSERT INTO tasks (id, meeting_id, title, status) VALUES ('t1', 'm1', 'x', 'open')",
                [],
            )
            .unwrap();
        s.delete_meeting("m1").unwrap();
        let meeting: Option<String> = s
            .conn()
            .query_row("SELECT meeting_id FROM tasks WHERE id = 't1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(meeting, None);
    }

    fn pragma_i64(s: &Store, name: &str) -> i64 {
        s.conn()
            .query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
            .unwrap()
    }

    fn check_pragmas(s: &Store) {
        assert_eq!(pragma_i64(s, "secure_delete"), 1, "secure_delete");
        assert_eq!(pragma_i64(s, "synchronous"), 1, "synchronous NORMAL");
        assert_eq!(pragma_i64(s, "busy_timeout"), 5000, "busy_timeout");
        assert_eq!(pragma_i64(s, "foreign_keys"), 1, "foreign_keys");
    }

    #[test]
    fn a_time_beyond_the_database_integer_range_is_refused_and_nothing_is_stored() {
        assert_eq!(
            ms_to_sql(1_700_000_000_000).expect("normal time"),
            1_700_000_000_000
        );
        assert!(matches!(
            ms_to_sql(u64::MAX),
            Err(StoreError::TimeOutOfRange(u64::MAX))
        ));
        let store = Store::open_in_memory().expect("memory store");
        assert!(matches!(
            store.insert_meeting("m1", "Too far", u64::MAX),
            Err(StoreError::TimeOutOfRange(_))
        ));
        assert_eq!(
            count(&store, "meetings"),
            0,
            "a refused time must not leave a row"
        );
    }

    #[test]
    fn memory_database_sets_every_pragma() {
        check_pragmas(&ready());
    }

    #[test]
    fn file_database_sets_every_pragma() {
        let path = temp_db_path("pragmas");
        {
            let s = Store::open(&path).unwrap();
            check_pragmas(&s);
        }
        remove_db(&path);
    }

    fn index_names(s: &Store) -> Vec<String> {
        let mut v: Vec<String> = s
            .conn()
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        v.sort();
        v
    }

    #[test]
    fn lookup_columns_are_indexed() {
        let s = ready();
        let names = index_names(&s);
        for want in [
            "summaries_by_meeting",
            "insights_by_meeting",
            "notes_by_meeting",
            "tasks_by_meeting",
            "jobs_by_meeting",
            "jobs_by_state_run_at",
        ] {
            assert!(names.iter().any(|n| n == want), "missing index {want}");
        }
        let plan: String = s
            .conn()
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM jobs WHERE state = 'queued' AND run_at_ms <= 5",
                [],
                |r| r.get(3),
            )
            .unwrap();
        assert!(plan.contains("jobs_by_state_run_at"), "plan: {plan}");
    }

    #[test]
    fn note_index_is_keyed_by_rowid_not_by_a_scanned_column() {
        let s = ready();
        let cols: Vec<String> = s
            .conn()
            .prepare("PRAGMA table_info(notes_fts)")
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(cols, vec!["text".to_string()]);
        for trigger in ["notes_fts_delete", "notes_fts_update"] {
            let sql: String = s
                .conn()
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
                    [trigger],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                sql.contains("rowid ="),
                "{trigger} must delete by rowid: {sql}"
            );
        }
    }

    #[test]
    fn deleting_one_of_two_identical_notes_keeps_the_other_searchable() {
        let s = ready();
        s.insert_note("n1", None, "same words", None, 1).unwrap();
        s.insert_note("n2", None, "same words", None, 2).unwrap();
        s.conn()
            .execute("DELETE FROM notes WHERE id = 'n1'", [])
            .unwrap();
        assert_eq!(s.search_notes("same").unwrap(), vec!["n2".to_string()]);
    }

    #[test]
    fn search_returns_at_most_the_limit() {
        let s = ready();
        for i in 0..60 {
            s.insert_note(&format!("n{i}"), None, "common word", None, i)
                .unwrap();
        }
        assert_eq!(
            s.search_notes("common").unwrap().len(),
            DEFAULT_SEARCH_LIMIT
        );
        assert_eq!(DEFAULT_SEARCH_LIMIT, 50);
        assert_eq!(s.search_notes_with_limit("common", 5).unwrap().len(), 5);
        assert_eq!(s.search_notes_with_limit("common", 500).unwrap().len(), 60);
        assert!(s.search_notes_with_limit("common", 0).unwrap().is_empty());
    }

    #[test]
    fn another_connection_cannot_delete_a_final_line() {
        let path = temp_db_path("second-conn");
        {
            let s = Store::open(&path).unwrap();
            s.insert_meeting("m1", "t", 0).unwrap();
            s.upsert_utterance(&utt("u1", SegmentStatus::Final, "hello"))
                .unwrap();
            // A plain connection has no guard function: the delete fails closed.
            let other = Connection::open(&path).unwrap();
            assert!(other
                .execute("DELETE FROM utterances WHERE id = 'u1'", [])
                .is_err());
            assert_eq!(count(&s, "utterances"), 1);
        }
        remove_db(&path);
    }

    const SECRET_LINE: &str = "the quarterly numbers are confidential";
    const SECRET_NOTE: &str = "private budget note for the board";

    fn contains(bytes: &[u8], needle: &str) -> bool {
        bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
    }

    fn fill_secret_meeting(s: &Store) {
        s.insert_meeting("m1", "Board call", 0).unwrap();
        for i in 0..20 {
            let mut line = utt(&format!("u{i}"), SegmentStatus::Final, SECRET_LINE);
            line.start_ms = i * 1_000;
            line.end_ms = i * 1_000 + 900;
            s.upsert_utterance(&line).unwrap();
        }
        s.insert_note("n1", Some("m1"), SECRET_NOTE, None, 1)
            .unwrap();
    }

    #[test]
    fn deleted_meeting_text_is_gone_from_the_database_and_wal_files() {
        let path = temp_db_path("scrub");
        let wal = format!("{}-wal", path.display());
        {
            let s = Store::open(&path).unwrap();
            fill_secret_meeting(&s);
            let before_db = std::fs::read(&path).unwrap_or_default();
            let before_wal = std::fs::read(&wal).unwrap_or_default();
            assert!(
                contains(&before_db, SECRET_LINE) || contains(&before_wal, SECRET_LINE),
                "test precondition: the line text is on disk before delete"
            );
            assert!(
                contains(&before_db, SECRET_NOTE) || contains(&before_wal, SECRET_NOTE),
                "test precondition: the note text is on disk before delete"
            );

            let outcome = s.delete_meeting("m1").unwrap();

            assert_eq!(
                outcome,
                MeetingDeletion {
                    deleted: true,
                    scrubbed: true
                }
            );
            // The store connection is still open: only the checkpoint in
            // delete_meeting can have emptied the log.
            let db = std::fs::read(&path).unwrap();
            let log = std::fs::read(&wal).unwrap_or_default();
            for text in [SECRET_LINE, SECRET_NOTE] {
                assert!(!contains(&db, text), "{text:?} left in the database file");
                assert!(!contains(&log, text), "{text:?} left in the -wal file");
            }
        }
        remove_db(&path);
    }

    const MARKER: &str = "marker words of one meeting";

    /// Meeting `m1` with a final line and three journal events that hold the
    /// marker text.
    fn fill_marked_meeting(s: &Store) {
        s.insert_meeting("m1", "t", 0).unwrap();
        s.upsert_utterance(&utt("u1", SegmentStatus::Final, "hello"))
            .unwrap();
        for sequence in 1..=3 {
            s.journal()
                .append(&event(&format!("e{sequence}"), "m1", sequence, MARKER))
                .unwrap();
        }
    }

    fn pending(conn: &Connection) -> bool {
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM settings WHERE key = ?1)",
            [CLEANUP_PENDING_KEY],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn marker_on_disk(db: &std::path::Path) -> bool {
        [db.to_path_buf(), wal_of(db)]
            .iter()
            .any(|file| contains(&bytes(file).unwrap_or_default(), MARKER))
    }

    fn failing_rebuild(_: &Connection) -> rusqlite::Result<()> {
        Err(rusqlite::Error::InvalidQuery)
    }

    #[test]
    fn a_failed_meeting_delete_keeps_lines_and_journal_rows() {
        let s = final_store();
        s.journal()
            .append(&event("e1", "m1", 1, "journal line"))
            .unwrap();
        // The delete of the jobs runs after the delete of the journal rows.
        s.conn().execute_batch("DROP TABLE jobs").unwrap();

        assert!(s.delete_meeting("m1").is_err());

        assert_eq!(count(&s, "utterances"), 1);
        assert_eq!(count(&s, "events"), 1);
        assert!(!pending(s.conn()));
        assert!(s.conn().execute("DELETE FROM utterances", []).is_err());
        assert!(s.conn().execute("DELETE FROM events", []).is_err());
    }

    #[test]
    fn journal_rows_without_a_meeting_are_removed() {
        let dir = TempDir::new();
        let s = Store::open(dir.db()).unwrap();
        for sequence in 1..=3 {
            s.journal()
                .append(&event(&format!("e{sequence}"), "m9", sequence, MARKER))
                .unwrap();
        }
        assert!(marker_on_disk(&dir.db()), "precondition");

        let outcome = s.delete_meeting("m9").unwrap();

        assert_eq!(
            outcome,
            MeetingDeletion {
                deleted: false,
                scrubbed: true
            }
        );
        assert_eq!(count(&s, "events"), 0);
        assert!(!marker_on_disk(&dir.db()));
    }

    #[test]
    fn a_final_line_cannot_be_replaced_through_another_id_or_a_row_number() {
        let dir = TempDir::new();
        let s = Store::open(dir.db()).unwrap();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.upsert_utterance(&utt("u1", SegmentStatus::Final, "final line"))
            .unwrap();
        s.upsert_utterance(&utt("u2", SegmentStatus::Stable, "second line"))
            .unwrap();
        let lines = |s: &Store| -> Vec<String> {
            let mut stmt = s
                .conn()
                .prepare(
                    "SELECT concat_ws('|', id, meeting_id, source_id, speaker_id, start_ms,
                         end_ms, status, language, revision, text)
                     FROM utterances ORDER BY id",
                )
                .unwrap();
            let found = stmt.query_map([], |r| r.get(0)).unwrap();
            found.collect::<Result<_, _>>().unwrap()
        };
        let stored = lines(&s);
        let statements = [
            "UPDATE utterances SET text = 'rewritten' WHERE id = 'u1'".to_string(),
            raw_utterance_sql("INSERT OR REPLACE", "u1", "final", 9, "rewritten"),
            "DELETE FROM utterances WHERE id = 'u1'".to_string(),
            "UPDATE OR REPLACE utterances SET id = 'u1' WHERE id = 'u2'".to_string(),
            "UPDATE utterances SET id = 'unused' WHERE id = 'u2'".to_string(),
            "INSERT OR REPLACE INTO utterances (rowid, id, meeting_id, source_id, start_ms,
                 end_ms, status, language, revision, text)
             VALUES (1, 'u3', 'm1', 'incoming', 0, 1000, 'final', 'en', 1, 'rewritten')"
                .to_string(),
            "UPDATE OR REPLACE utterances SET rowid = 1 WHERE id = 'u2'".to_string(),
        ];
        let other = plain(&dir.db());

        for sql in &statements {
            assert!(s.conn().execute(sql, []).is_err(), "store: {sql}");
            assert_eq!(lines(&s), stored, "store: {sql}");
            assert!(other.execute(sql, []).is_err(), "plain: {sql}");
            assert_eq!(lines(&s), stored, "plain: {sql}");
        }
    }

    #[test]
    fn a_cleanup_left_by_an_exit_runs_at_the_next_open() {
        for with_reader in [false, true] {
            let dir = TempDir::new();
            let db = dir.db();
            {
                let s = Store::open(&db).unwrap();
                fill_marked_meeting(&s);
                s.commit_meeting_delete("m1").unwrap();
                // An exit: the log is not moved into the file.
                s.conn()
                    .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
                    .unwrap();
            }
            assert!(marker_on_disk(&db), "precondition");
            assert!(pending(&leaves_its_log(&db)), "precondition");
            let reader = with_reader.then(|| {
                let reader = leaves_its_log(&db);
                reader
                    .execute_batch("BEGIN; SELECT count(*) FROM meetings;")
                    .unwrap();
                reader
            });

            let mut s = Store::open(&db).unwrap_or_else(|e| panic!("{with_reader}: {e}"));

            if let Some(reader) = reader {
                assert!(pending(s.conn()), "a blocked cleanup stays pending");
                reader.execute_batch("COMMIT").unwrap();
                drop(s);
                s = Store::open(&db).unwrap();
            }
            assert!(!marker_on_disk(&db), "reader {with_reader}");
            assert!(!pending(s.conn()), "reader {with_reader}");
        }
    }

    #[test]
    fn a_rebuild_that_fails_is_reported_and_tried_again() {
        let dir = TempDir::new();
        let db = dir.db();
        {
            let s = Store::open_with(&db, MIGRATIONS, 1, failing_rebuild).unwrap();
            fill_marked_meeting(&s);

            let outcome = s.delete_meeting_with("m1", failing_rebuild).unwrap();

            assert_eq!(
                outcome,
                MeetingDeletion {
                    deleted: true,
                    scrubbed: false
                }
            );
            assert!(pending(s.conn()));
        }
        {
            let s = Store::open_with(&db, MIGRATIONS, 2, failing_rebuild).unwrap();
            assert!(pending(s.conn()), "the cleanup stays pending");
        }

        let s = Store::open(&db).unwrap();

        assert!(!marker_on_disk(&db));
        assert!(!pending(s.conn()));
    }

    #[test]
    fn delete_meeting_reports_when_a_reader_stops_the_wal_scrub() {
        let path = temp_db_path("scrub-busy");
        {
            let s = Store::open(&path).unwrap();
            s.conn().busy_timeout(Duration::from_millis(10)).unwrap();
            fill_secret_meeting(&s);
            for sequence in 1..=3 {
                s.journal()
                    .append(&event(&format!("e{sequence}"), "m1", sequence, MARKER))
                    .unwrap();
            }
            assert!(!pending(s.conn()));
            // A reader that started before the delete pins the old log frames.
            let reader = Connection::open(&path).unwrap();
            reader
                .execute_batch("BEGIN; SELECT count(*) FROM meetings;")
                .unwrap();

            let outcome = s.delete_meeting("m1").unwrap();

            assert!(outcome.deleted);
            assert!(!outcome.scrubbed, "the log cannot be truncated yet");
            assert!(pending(s.conn()));
            reader.execute_batch("COMMIT;").unwrap();
            assert!(s.scrub().unwrap(), "the retry succeeds");
            assert!(!pending(s.conn()));
            assert!(!marker_on_disk(&path));
        }
        remove_db(&path);
    }

    #[test]
    fn memory_store_delete_has_nothing_to_scrub_and_says_scrubbed() {
        let s = ready();
        s.insert_meeting("m1", "t", 0).unwrap();
        assert_eq!(
            s.delete_meeting("m1").unwrap(),
            MeetingDeletion {
                deleted: true,
                scrubbed: true
            }
        );
    }

    #[cfg(unix)]
    use crate::testing::mode_of;

    #[cfg(unix)]
    #[test]
    fn a_new_store_gets_a_private_directory_and_owner_only_files() {
        let base = temp_db_path("private-dir");
        let dir = base.with_extension("d").join("interlo");
        let path = dir.join("store.db");
        {
            let s = Store::open(&path).unwrap();
            s.insert_meeting("m1", "t", 0).unwrap();

            assert_eq!(mode_of(&dir), 0o700, "directory");
            assert_eq!(mode_of(&path), 0o600, "database file");
            let wal = format!("{}-wal", path.display());
            let shm = format!("{}-shm", path.display());
            assert_eq!(mode_of(std::path::Path::new(&wal)), 0o600, "-wal file");
            assert_eq!(mode_of(std::path::Path::new(&shm)), 0o600, "-shm file");
        }
        let _ = std::fs::remove_dir_all(base.with_extension("d"));
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_world_readable_store_file_is_tightened_on_open() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_db_path("loose");
        std::fs::write(&path, b"").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        {
            let _s = Store::open(&path).unwrap();
            assert_eq!(mode_of(&path), 0o600);
        }
        remove_db(&path);
    }

    #[test]
    fn reopen_keeps_rows_and_adds_no_record() {
        use crate::testing::{records, schema_of, TempDir};
        let rows = |s: &Store| -> Vec<String> {
            let mut stmt = s
                .conn()
                .prepare(
                    "SELECT 'meeting ' || id || ' ' || title || ' ' || started_ms FROM meetings
                     UNION ALL
                     SELECT 'line ' || id || ' ' || status || ' ' || text FROM utterances
                     UNION ALL
                     SELECT 'event ' || event_id || ' ' || session_id || ' ' || sequence
                         || ' ' || payload FROM events",
                )
                .unwrap();
            let found = stmt.query_map([], |r| r.get(0)).unwrap();
            found.collect::<Result<_, _>>().unwrap()
        };
        let dir = TempDir::new();
        let first = Store::open(dir.db()).unwrap();
        first.insert_meeting("m1", "t", 7).unwrap();
        first
            .upsert_utterance(&utt("u1", SegmentStatus::Final, "hello"))
            .unwrap();
        first
            .journal()
            .append(&crate::testing::event("e1", "m1", 1, "hello"))
            .unwrap();
        let before = rows(&first);
        assert_eq!(before.len(), 3, "precondition");
        let objects = schema_of(first.conn());
        drop(first);

        let second = Store::open(dir.db()).unwrap();

        assert_eq!(rows(&second), before);
        assert_eq!(second.schema_version().unwrap(), 1);
        assert_eq!(records(second.conn()).len(), 1);
        assert_eq!(schema_of(second.conn()), objects);
    }

    #[test]
    fn purge_is_a_stub() {
        let s = ready();
        assert!(matches!(
            s.purge_meetings_before(0),
            Err(StoreError::NotImplemented(_))
        ));
    }
}
