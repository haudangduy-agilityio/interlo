//! Append-only event journal in SQLite (handoff: "crash-safe session persistence,
//! SQLite WAL and append-only segment/event journal").
//!
//! Rules: `event_id` is the idempotency key. `(session_id, sequence)` is unique.
//! Rows are never updated. They are deleted only through `delete_session`,
//! which removes a whole session and scrubs the freed pages (`secure_delete`),
//! so deleted transcript text does not stay in the database file.
//!
//! The database carries `PRAGMA user_version` (`JOURNAL_SCHEMA_VERSION`, now 1).
//! Opening a version 0 database stamps it 1; a higher version is refused with
//! `JournalError::UnsupportedVersion` before anything is changed.

use crate::protocol::Envelope;
use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

/// Mode of a database directory created by `prepare_private_db`: owner only.
#[cfg(unix)]
const PRIVATE_DIR_MODE: u32 = 0o700;
/// Mode of a database file and its `-wal` and `-shm` files: owner only.
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

/// How long a statement waits for a locked database, in milliseconds.
/// Starting value; a measurement under real load may change it.
const BUSY_TIMEOUT_MS: u64 = 5_000;

/// `PRAGMA user_version` of a journal this build writes. Schema v1 may change
/// freely until the first public tag; after that every change needs a migration (D-59).
pub const JOURNAL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sequence {sequence} already used in session {session_id} by another event")]
    SequenceConflict { session_id: String, sequence: u64 },
    #[error("journal schema version {0} is newer than this app supports")]
    UnsupportedVersion(u32),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sequence {0} does not fit a database integer")]
    SequenceOutOfRange(u64),
}

/// SQLite integers are signed 64-bit. A sequence above that range cannot be stored.
fn sequence_to_sql(sequence: u64) -> Result<i64, JournalError> {
    i64::try_from(sequence).map_err(|_| JournalError::SequenceOutOfRange(sequence))
}

/// Read a sequence column back as u64.
fn sequence_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

/// Create the parent directory of a database file (mode 0700 when this call
/// creates it) and the file itself (mode 0600), before SQLite opens it. A file
/// that exists is tightened to 0600. SQLite gives `-wal` and `-shm` the mode
/// of the main file, so they start at 0600 too. Directories that already exist
/// are left as they are: they may be shared (for example the temp directory).
/// On other platforms this only creates the directory.
pub fn prepare_private_db(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if !parent.exists() {
            create_private_dir(parent)?;
        }
    }
    create_private_file(path)
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIR_MODE)
        .create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(PRIVATE_FILE_MODE)
        .open(path)?;
    // `mode` above does not change a file that already existed.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(PRIVATE_FILE_MODE))
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map(|_| ())
}

/// Set the `-wal` and `-shm` files that exist next to `path` to 0600. SQLite
/// already creates them with the mode of the main file; this keeps them
/// private when the main file was made by an older build. Missing files are
/// skipped.
#[cfg(unix)]
pub fn tighten_db_sidecars(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for suffix in ["-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        match std::fs::set_permissions(&name, std::fs::Permissions::from_mode(PRIVATE_FILE_MODE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn tighten_db_sidecars(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Checkpoint the write-ahead log and truncate it to zero bytes, so old
/// copies of deleted text do not stay in it (ADR 0008). Returns `Ok(true)` when
/// the log is empty afterwards (also for a database without a log) and
/// `Ok(false)` when another connection's read lock stopped the checkpoint: the
/// text may still be in the log until a later call succeeds. The truncate
/// releases the log's bytes; it does not overwrite the freed disk blocks.
pub fn scrub_wal(conn: &Connection) -> Result<bool, rusqlite::Error> {
    // The row is (busy, log frames, checkpointed frames); busy is 1 when the
    // checkpoint could not finish.
    let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
    Ok(busy == 0)
}

fn read_user_version(conn: &Connection) -> Result<u32, JournalError> {
    let raw: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    // user_version is a signed 32-bit field; a negative value is not one we wrote.
    Ok(u32::try_from(raw).unwrap_or(u32::MAX))
}

fn refuse_newer_version(conn: &Connection) -> Result<(), JournalError> {
    match read_user_version(conn)? {
        v if v > JOURNAL_SCHEMA_VERSION => Err(JournalError::UnsupportedVersion(v)),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    Inserted,
    Duplicate,
}

/// What `Journal::delete_session` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionDeletion {
    /// Rows removed from the journal.
    pub removed: usize,
    /// True when the write-ahead log was truncated, so no copy of the deleted
    /// text stays in it. False means "scrub again later" (`Journal::scrub`).
    pub scrubbed: bool,
}

pub struct Journal {
    conn: Connection,
}

impl Journal {
    pub fn open_in_memory() -> Result<Self, JournalError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    /// Open (or create) a journal file. The directory and the file are made
    /// owner-only first (see `prepare_private_db`).
    pub fn open(path: &str) -> Result<Self, JournalError> {
        let file = Path::new(path);
        let is_file = path != ":memory:";
        if is_file {
            prepare_private_db(file)?;
        }
        let conn = Connection::open(path)?;
        // Refuse before any pragma or table touches a newer database.
        refuse_newer_version(&conn)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let journal = Self::init(conn)?;
        if is_file {
            tighten_db_sidecars(file)?;
        }
        Ok(journal)
    }

    fn init(conn: Connection) -> Result<Self, JournalError> {
        refuse_newer_version(&conn)?;
        conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                event_id    TEXT PRIMARY KEY,
                session_id  TEXT NOT NULL,
                sequence    INTEGER NOT NULL,
                schema_version TEXT NOT NULL,
                event_type  TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                payload     TEXT NOT NULL,
                UNIQUE (session_id, sequence)
            );",
        )?;
        // Version 0 is a new database or one made before versioning; both now
        // hold the version 1 table, so stamp them.
        if read_user_version(&conn)? < JOURNAL_SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", JOURNAL_SCHEMA_VERSION)?;
        }
        Ok(Self { conn })
    }

    /// The `PRAGMA user_version` of this database.
    pub fn schema_version(&self) -> Result<u32, JournalError> {
        read_user_version(&self.conn)
    }

    /// Insert first. A UNIQUE violation is the duplicate check: the same
    /// event again is `Duplicate`; a reused `event_id` or sequence that
    /// belongs to another event is a `SequenceConflict`.
    pub fn append(&self, env: &Envelope) -> Result<AppendOutcome, JournalError> {
        let payload = serde_json::to_string(&env.payload)?;
        let res = self
            .conn
            .prepare_cached(
                "INSERT INTO events (event_id, session_id, sequence, schema_version, event_type, occurred_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?
            .execute(params![
                env.event_id,
                env.session_id,
                sequence_to_sql(env.sequence)?,
                env.schema_version,
                env.event_type,
                env.occurred_at,
                payload
            ]);
        match res {
            Ok(_) => Ok(AppendOutcome::Inserted),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == ErrorCode::ConstraintViolation =>
            {
                self.classify_conflict(env)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn classify_conflict(&self, env: &Envelope) -> Result<AppendOutcome, JournalError> {
        let existing: Option<(String, u64)> = self
            .conn
            .prepare_cached("SELECT session_id, sequence FROM events WHERE event_id = ?1")?
            .query_row(params![env.event_id], |r| {
                Ok((r.get(0)?, sequence_from_row(r, 1)?))
            })
            .optional()?;
        match existing {
            Some((sid, seq)) if sid == env.session_id && seq == env.sequence => {
                Ok(AppendOutcome::Duplicate)
            }
            _ => Err(JournalError::SequenceConflict {
                session_id: env.session_id.clone(),
                sequence: env.sequence,
            }),
        }
    }

    /// Delete every row of one session. Freed pages are zeroed
    /// (`secure_delete`), then the write-ahead log is checkpointed and
    /// truncated so no old copy of the text stays in the log. The checkpoint
    /// cannot finish while another connection holds a read lock: then
    /// `scrubbed` is false and the caller must call `scrub` again later.
    pub fn delete_session(&self, session_id: &str) -> Result<SessionDeletion, JournalError> {
        let removed = self
            .conn
            .prepare_cached("DELETE FROM events WHERE session_id = ?1")?
            .execute(params![session_id])?;
        // A failed checkpoint is reported as `scrubbed: false`; the rows are
        // already gone.
        let scrubbed = scrub_wal(&self.conn).unwrap_or(false);
        Ok(SessionDeletion { removed, scrubbed })
    }

    /// Checkpoint and truncate the write-ahead log again. `Ok(true)` when the
    /// log is empty afterwards.
    pub fn scrub(&self) -> Result<bool, JournalError> {
        Ok(scrub_wal(&self.conn)?)
    }

    /// Events of one session, ordered by sequence. Used for replay.
    pub fn load(&self, session_id: &str) -> Result<Vec<Envelope>, JournalError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT event_id, session_id, sequence, schema_version, event_type, occurred_at, payload
             FROM events WHERE session_id = ?1 ORDER BY sequence ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                sequence_from_row(r, 2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (event_id, session_id, sequence, schema_version, event_type, occurred_at, payload) =
                row?;
            out.push(Envelope {
                schema_version,
                event_id,
                session_id,
                event_type,
                sequence,
                occurred_at,
                payload: serde_json::from_str(&payload)?,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{EventPayload, Language, SegmentStatus, TranscriptSegment};
    use std::path::PathBuf;

    const SECRET: &str = "the quarterly numbers are confidential";

    fn event(id: &str, session: &str, seq: u64, text: &str) -> Envelope {
        let payload = EventPayload::TranscriptSegmentUpdated(TranscriptSegment {
            segment_id: format!("seg_{seq}"),
            source_id: "incoming".into(),
            revision: 1,
            start_ms: 0,
            end_ms: 1000,
            status: SegmentStatus::Final,
            language: Language::En,
            text: text.into(),
        });
        Envelope::new(id, session, seq, "2026-11-13T00:00:00.000Z", &payload).expect("envelope")
    }

    fn pragma_i64(journal: &Journal, name: &str) -> i64 {
        journal
            .conn
            .query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
            .expect("pragma")
    }

    struct TempDb(PathBuf);

    impl TempDb {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir()
                .join(format!("meeting-core-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn db(&self) -> String {
            self.0.join("journal.db").to_string_lossy().into_owned()
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn both_open_paths_set_secure_delete_synchronous_and_busy_timeout() {
        let dir = TempDb::new("pragmas");
        let file = Journal::open(&dir.db()).expect("file journal");
        let memory = Journal::open_in_memory().expect("memory journal");
        for journal in [&file, &memory] {
            assert_eq!(pragma_i64(journal, "secure_delete"), 1);
            assert_eq!(pragma_i64(journal, "synchronous"), 1, "NORMAL");
            assert_eq!(pragma_i64(journal, "busy_timeout"), 5_000);
        }
    }

    fn raw_db(path: &str, version: u32, with_events: bool) {
        let conn = Connection::open(path).expect("raw db");
        if with_events {
            conn.execute_batch(
                "CREATE TABLE events (
                    event_id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL, schema_version TEXT NOT NULL,
                    event_type TEXT NOT NULL, occurred_at TEXT NOT NULL,
                    payload TEXT NOT NULL, UNIQUE (session_id, sequence));",
            )
            .expect("events table");
        }
        conn.pragma_update(None, "user_version", version)
            .expect("set version");
    }

    #[test]
    fn a_sequence_beyond_the_database_integer_range_is_refused_not_wrapped() {
        assert_eq!(sequence_to_sql(7).expect("small"), 7);
        assert_eq!(
            sequence_to_sql(i64::MAX as u64).expect("largest that fits"),
            i64::MAX
        );
        assert!(matches!(
            sequence_to_sql(i64::MAX as u64 + 1),
            Err(JournalError::SequenceOutOfRange(_))
        ));
        assert!(matches!(
            sequence_to_sql(u64::MAX),
            Err(JournalError::SequenceOutOfRange(u64::MAX))
        ));
    }

    #[test]
    fn a_negative_stored_sequence_is_an_error_not_a_huge_number() {
        let journal = Journal::open_in_memory().expect("memory journal");
        let ok: u64 = journal
            .conn
            .query_row("SELECT 42", [], |r| sequence_from_row(r, 0))
            .expect("positive value");
        assert_eq!(ok, 42);
        let negative = journal
            .conn
            .query_row("SELECT -5", [], |r| sequence_from_row(r, 0));
        assert!(matches!(
            negative,
            Err(rusqlite::Error::IntegralValueOutOfRange(0, -5))
        ));
    }

    #[test]
    fn a_new_database_has_schema_version_1() {
        let dir = TempDb::new("version-new");
        assert_eq!(JOURNAL_SCHEMA_VERSION, 1);
        let file = Journal::open(&dir.db()).expect("file journal");
        assert_eq!(file.schema_version().expect("version"), 1);
        assert_eq!(pragma_i64(&file, "user_version"), 1);
        let memory = Journal::open_in_memory().expect("memory journal");
        assert_eq!(memory.schema_version().expect("version"), 1);
        drop(file);
        let reopened = Journal::open(&dir.db()).expect("reopen");
        assert_eq!(reopened.schema_version().expect("version"), 1);
    }

    #[test]
    fn an_existing_version_0_database_is_upgraded_to_1_and_keeps_its_rows() {
        let dir = TempDb::new("version-v0");
        raw_db(&dir.db(), 0, true);
        {
            let conn = Connection::open(dir.db()).expect("raw db");
            conn.execute(
                "INSERT INTO events VALUES ('e1','s1',1,'1','transcript.segment.updated','2026-11-13T00:00:00.000Z','{}')",
                [],
            )
            .expect("old row");
        }
        let journal = Journal::open(&dir.db()).expect("open v0");
        assert_eq!(journal.schema_version().expect("version"), 1);
        let rows: i64 = journal
            .conn
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(rows, 1, "the old row stays");
    }

    #[test]
    fn a_database_with_a_newer_version_is_refused_and_left_untouched() {
        let dir = TempDb::new("version-99");
        raw_db(&dir.db(), 99, false);
        let err = Journal::open(&dir.db()).err().expect("must refuse");
        assert!(
            matches!(err, JournalError::UnsupportedVersion(99)),
            "{err:?}"
        );
        let conn = Connection::open(dir.db()).expect("raw db");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .expect("version");
        assert_eq!(version, 99, "version is not lowered");
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'events'",
                [],
                |r| r.get(0),
            )
            .expect("tables");
        assert_eq!(tables, 0, "nothing is created in a refused database");
    }

    #[test]
    fn the_first_unsupported_version_is_2() {
        let dir = TempDb::new("version-2");
        raw_db(&dir.db(), 2, true);
        assert!(matches!(
            Journal::open(&dir.db()).err().expect("must refuse"),
            JournalError::UnsupportedVersion(2)
        ));
    }

    #[test]
    fn delete_session_removes_every_row_of_that_session_only() {
        let journal = Journal::open_in_memory().expect("journal");
        for seq in 1..=3 {
            journal
                .append(&event(&format!("a{seq}"), "session_a", seq, SECRET))
                .expect("append a");
        }
        journal
            .append(&event("b1", "session_b", 1, "keep this line"))
            .expect("append b");

        let deletion = journal.delete_session("session_a").expect("delete");
        assert_eq!(deletion.removed, 3);
        assert!(
            deletion.scrubbed,
            "an in-memory journal has no log to scrub"
        );

        assert!(journal.load("session_a").expect("load a").is_empty());
        let left: i64 = journal
            .conn
            .query_row(
                "SELECT count(*) FROM events WHERE payload LIKE '%confidential%'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(left, 0, "no transcript text is left in the table");
        assert_eq!(journal.load("session_b").expect("load b").len(), 1);
        assert_eq!(
            journal.delete_session("session_a").expect("again").removed,
            0
        );
    }

    #[test]
    fn deleted_text_is_gone_from_the_database_files() {
        let dir = TempDb::new("scrub");
        let journal = Journal::open(&dir.db()).expect("journal");
        for seq in 1..=20 {
            journal
                .append(&event(&format!("a{seq}"), "session_a", seq, SECRET))
                .expect("append");
        }
        let bytes_before = std::fs::read(dir.db()).unwrap_or_default();
        let wal = format!("{}-wal", dir.db());
        let wal_before = std::fs::read(&wal).unwrap_or_default();
        let contains = |bytes: &[u8]| bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes());
        assert!(
            contains(&bytes_before) || contains(&wal_before),
            "test precondition: the text is on disk before delete"
        );

        let deletion = journal.delete_session("session_a").expect("delete");
        assert!(deletion.scrubbed, "no other connection holds a read lock");

        let main = std::fs::read(dir.db()).expect("db file");
        let wal_after = std::fs::read(&wal).unwrap_or_default();
        assert!(!contains(&main), "text left in the database file");
        assert!(!contains(&wal_after), "text left in the write-ahead log");
    }

    #[test]
    fn delete_session_reports_when_a_reader_stops_the_wal_scrub() {
        let dir = TempDb::new("scrub-busy");
        let journal = Journal::open(&dir.db()).expect("journal");
        journal
            .conn
            .busy_timeout(Duration::from_millis(10))
            .expect("short wait");
        journal
            .append(&event("a1", "session_a", 1, SECRET))
            .expect("append");
        // A reader that started before the delete pins the old log frames.
        let reader = Connection::open(dir.db()).expect("reader");
        reader
            .execute_batch("BEGIN; SELECT count(*) FROM events;")
            .expect("read snapshot");

        let deletion = journal.delete_session("session_a").expect("delete");
        assert_eq!(deletion.removed, 1);
        assert!(!deletion.scrubbed, "the log cannot be truncated yet");

        reader.execute_batch("COMMIT;").expect("release reader");
        assert!(journal.scrub().expect("scrub again"), "retry succeeds");
    }

    #[cfg(unix)]
    fn mode_of(path: &str) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn a_new_journal_gets_a_private_directory_and_owner_only_files() {
        let dir = TempDb::new("private");
        let nested = dir.0.join("data").join("interlo");
        let db = nested.join("journal.db").to_string_lossy().into_owned();
        let journal = Journal::open(&db).expect("journal");
        journal
            .append(&event("a1", "session_a", 1, SECRET))
            .expect("append");

        assert_eq!(mode_of(&nested.to_string_lossy()), 0o700, "directory");
        assert_eq!(mode_of(&db), 0o600, "database file");
        assert_eq!(mode_of(&format!("{db}-wal")), 0o600, "write-ahead log");
        assert_eq!(mode_of(&format!("{db}-shm")), 0o600, "shared memory file");
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_world_readable_journal_file_is_tightened_on_open() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDb::new("tighten");
        raw_db(&dir.db(), 1, true);
        std::fs::set_permissions(dir.db(), std::fs::Permissions::from_mode(0o644)).expect("loosen");
        let _journal = Journal::open(&dir.db()).expect("journal");
        assert_eq!(mode_of(&dir.db()), 0o600);
    }

    #[test]
    fn same_event_id_in_another_session_is_a_conflict_not_a_duplicate() {
        let journal = Journal::open_in_memory().expect("journal");
        journal.append(&event("e1", "s1", 1, "x")).expect("first");
        assert!(matches!(
            journal.append(&event("e1", "s2", 1, "x")),
            Err(JournalError::SequenceConflict { .. })
        ));
        assert!(matches!(
            journal.append(&event("e1", "s1", 2, "x")),
            Err(JournalError::SequenceConflict { .. })
        ));
        assert_eq!(
            journal.append(&event("e1", "s1", 1, "x")).expect("same"),
            AppendOutcome::Duplicate
        );
    }
}
