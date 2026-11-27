//! Append-only event journal in the database file of the store (D-134).
//!
//! Rules: `event_id` is the idempotency key and `(session_id, sequence)` is
//! unique. A session id is a meeting id. Rows are never updated, and triggers
//! abort a raw `UPDATE`, `DELETE` or `INSERT OR REPLACE`. The one call that
//! removes rows is `Store::delete_meeting`.

use meeting_core::protocol::Envelope;
use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sequence {sequence} already used in session {session_id} by another event")]
    SequenceConflict { session_id: String, sequence: u64 },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    Inserted,
    Duplicate,
}

/// The journal of one store. Made by `Store::journal`.
pub struct Journal<'a> {
    conn: &'a Connection,
}

impl<'a> Journal<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert first. A constraint failure is the duplicate check: the same
    /// event again is `Duplicate`; a reused `event_id` or sequence that belongs
    /// to another event is a `SequenceConflict`.
    pub fn append(&self, env: &Envelope) -> Result<AppendOutcome, JournalError> {
        let payload = serde_json::to_string(&env.payload)?;
        let inserted = self
            .conn
            .prepare_cached(
                "INSERT INTO events
                     (event_id, session_id, sequence, schema_version, event_type, occurred_at, payload)
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
        match inserted {
            Ok(_) => Ok(AppendOutcome::Inserted),
            Err(e) if e.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) => {
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
            Some((session, sequence)) if session == env.session_id && sequence == env.sequence => {
                Ok(AppendOutcome::Duplicate)
            }
            _ => Err(JournalError::SequenceConflict {
                session_id: env.session_id.clone(),
                sequence: env.sequence,
            }),
        }
    }

    /// Events of one session, ordered by sequence. Used for replay.
    pub fn load(&self, session_id: &str) -> Result<Vec<Envelope>, JournalError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT event_id, session_id, sequence, schema_version, event_type, occurred_at, payload
             FROM events WHERE session_id = ?1 ORDER BY sequence ASC",
        )?;
        let mut rows = stmt.query(params![session_id])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            let payload: String = row.get(6)?;
            events.push(Envelope {
                event_id: row.get(0)?,
                session_id: row.get(1)?,
                sequence: sequence_from_row(row, 2)?,
                schema_version: row.get(3)?,
                event_type: row.get(4)?,
                occurred_at: row.get(5)?,
                payload: serde_json::from_str(&payload)?,
            });
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{count, event, plain, TempDir};
    use crate::Store;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    /// Every column of every journal row, as one text per row.
    fn rows(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT concat_ws('|', event_id, session_id, sequence, schema_version,
                     event_type, occurred_at, payload)
                 FROM events ORDER BY session_id, sequence",
            )
            .unwrap();
        let found = stmt.query_map([], |r| r.get(0)).unwrap();
        found.collect::<Result<_, _>>().unwrap()
    }

    #[test]
    fn journal_and_tables_live_in_one_file() {
        let dir = TempDir::new();
        let s = Store::open(dir.db()).unwrap();
        s.insert_meeting("m1", "t", 0).unwrap();
        s.journal().append(&event("e1", "m1", 1, "x")).unwrap();

        let other = plain(&dir.db());
        assert_eq!(count(&other, "meetings"), 1);
        assert_eq!(count(&other, "events"), 1);
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["store.db", "store.db-shm", "store.db-wal"]);
    }

    #[test]
    fn the_same_event_twice_is_a_duplicate() {
        let s = store();
        let journal = s.journal();
        let first = event("e1", "s1", 1, "first text");
        assert_eq!(journal.append(&first).unwrap(), AppendOutcome::Inserted);

        assert_eq!(journal.append(&first).unwrap(), AppendOutcome::Duplicate);
        assert_eq!(
            journal
                .append(&event("e1", "s1", 1, "another text"))
                .unwrap(),
            AppendOutcome::Duplicate
        );

        assert_eq!(journal.load("s1").unwrap(), [first]);
        assert_eq!(count(s.conn(), "events"), 1);
    }

    #[test]
    fn a_reused_sequence_or_event_id_is_a_conflict() {
        let s = store();
        let journal = s.journal();
        journal.append(&event("e1", "s1", 1, "x")).unwrap();
        let stored = rows(s.conn());

        for (case, other) in [
            ("sequence used by another event", event("e2", "s1", 1, "x")),
            ("event id in another session", event("e1", "s2", 1, "x")),
            ("event id at another sequence", event("e1", "s1", 2, "x")),
        ] {
            assert!(
                matches!(
                    journal.append(&other),
                    Err(JournalError::SequenceConflict { .. })
                ),
                "{case}"
            );
        }

        assert_eq!(rows(s.conn()), stored);
        assert_eq!(
            journal.append(&event("e1", "s1", 1, "x")).unwrap(),
            AppendOutcome::Duplicate
        );
    }

    #[test]
    fn journal_rejects_sequence_reuse_by_a_different_event() {
        let s = store();
        let journal = s.journal();
        let a = event("evt_a", "session_t", 1, "a");
        let b = event("evt_b", "session_t", 1, "b");
        journal.append(&a).unwrap();
        assert!(journal.append(&b).is_err());
    }

    #[test]
    fn journal_keeps_session_ids_apart() {
        let s = store();
        let journal = s.journal();
        journal
            .append(&event("evt_a", "session_t", 1, "a"))
            .unwrap();
        assert!(journal.load("session_other").unwrap().is_empty());
    }

    #[test]
    fn a_sequence_above_the_integer_range_is_refused_not_wrapped() {
        assert_eq!(sequence_to_sql(7).unwrap(), 7);
        assert_eq!(sequence_to_sql(i64::MAX as u64).unwrap(), i64::MAX);
        assert!(matches!(
            sequence_to_sql(i64::MAX as u64 + 1),
            Err(JournalError::SequenceOutOfRange(_))
        ));
        assert!(matches!(
            sequence_to_sql(u64::MAX),
            Err(JournalError::SequenceOutOfRange(u64::MAX))
        ));

        let s = store();
        let journal = s.journal();
        let ends = [
            event("e0", "s1", 0, "x"),
            event("e_max", "s1", i64::MAX as u64, "x"),
        ];
        for end in &ends {
            assert_eq!(journal.append(end).unwrap(), AppendOutcome::Inserted);
        }
        let above = i64::MAX as u64 + 1;
        assert!(matches!(
            journal.append(&event("e_above", "s1", above, "x")),
            Err(JournalError::SequenceOutOfRange(found)) if found == above
        ));
        assert_eq!(journal.load("s1").unwrap(), ends);
        assert_eq!(count(s.conn(), "events"), 2);
    }

    #[test]
    fn a_negative_stored_sequence_is_an_error() {
        let s = store();
        let ok: u64 = s
            .conn()
            .query_row("SELECT 42", [], |r| sequence_from_row(r, 0))
            .unwrap();
        assert_eq!(ok, 42);
        let negative = s
            .conn()
            .query_row("SELECT -5", [], |r| sequence_from_row(r, 0));
        assert!(matches!(
            negative,
            Err(rusqlite::Error::IntegralValueOutOfRange(0, -5))
        ));

        s.conn()
            .execute(
                "INSERT INTO events VALUES ('e1', 's1', -5, '0.1', 'a.type',
                     '2026-11-26T00:00:00.000Z', '{}')",
                [],
            )
            .unwrap();
        assert!(matches!(
            s.journal().load("s1"),
            Err(JournalError::Sqlite(
                rusqlite::Error::IntegralValueOutOfRange(_, -5)
            ))
        ));
    }

    #[test]
    fn raw_statements_cannot_change_or_remove_journal_rows() {
        const UPDATE: &str = "UPDATE events SET payload = '{}' WHERE event_id = 'e1'";
        const DELETE: &str = "DELETE FROM events WHERE event_id = 'e1'";
        const BY_ID: &str = "INSERT OR REPLACE INTO events VALUES
            ('e1', 's2', 9, '0.1', 'a.type', 'at', '{}')";
        const BY_SEQUENCE: &str = "INSERT OR REPLACE INTO events VALUES
            ('e2', 's1', 1, '0.1', 'a.type', 'at', '{}')";
        const BY_ROW_NUMBER: &str = "INSERT OR REPLACE INTO events (rowid, event_id, session_id,
            sequence, schema_version, event_type, occurred_at, payload) VALUES
            (1, 'e3', 's3', 3, '0.1', 'a.type', 'at', '{}')";
        const NULL_ID: &str = "INSERT INTO events VALUES
            (NULL, 's4', 4, '0.1', 'a.type', 'at', '{}')";
        let dir = TempDir::new();
        let s = Store::open(dir.db()).unwrap();
        s.journal()
            .append(&event("e1", "s1", 1, "kept text"))
            .unwrap();
        let stored = rows(s.conn());
        let other = plain(&dir.db());

        for sql in [UPDATE, DELETE, BY_ID, BY_SEQUENCE, BY_ROW_NUMBER, NULL_ID] {
            assert!(s.conn().execute(sql, []).is_err(), "store: {sql}");
            assert_eq!(rows(s.conn()), stored, "store: {sql}");
        }
        for sql in [DELETE, BY_ROW_NUMBER] {
            assert!(other.execute(sql, []).is_err(), "plain: {sql}");
            assert_eq!(rows(s.conn()), stored, "plain: {sql}");
        }
    }
}
