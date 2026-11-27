//! The schema version of the database file: the migrations that this build
//! holds, their records, and the rule that decides whether a file may be opened.

use crate::StoreError;
use rusqlite::types::Value;
use rusqlite::{params, Connection, Transaction, TransactionBehavior};

/// One numbered change of the schema.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Migration {
    pub version: i64,
    pub name: &'static str,
    /// True when a build that does not hold this migration must not open the
    /// database (D-125).
    pub incompatible: bool,
    pub sql: &'static str,
}

/// The migrations of this build, in version order.
pub(crate) const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "schema_v1",
    incompatible: false,
    sql: include_str!("../migrations/0001_schema_v1.sql"),
}];

/// Schema version that this build writes, stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 1;

/// What the version rule says about a database that may be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Found {
    /// The migrations above this version are pending. 0 for a new database.
    Behind(i64),
    /// Nothing to apply: the supported version, or a newer additive one.
    Ready,
}

/// Read the schema version and the migration records and apply the version
/// rule. Writes nothing. The caller holds a transaction, so that every read
/// sees one state of the file.
pub(crate) fn inspect(conn: &Connection, migrations: &[Migration]) -> Result<Found, StoreError> {
    let supported = migrations.last().map_or(0, |m| m.version);
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let unknown = || StoreError::UnknownSchema { found: version };
    if version == 0 {
        let used: bool =
            conn.query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master)", [], |r| {
                r.get(0)
            })?;
        return if used {
            Err(unknown())
        } else {
            Ok(Found::Behind(0))
        };
    }
    if version < 0 {
        return Err(unknown());
    }
    let records = read_records(conn)?.ok_or_else(unknown)?;
    let complete = i64::try_from(records.len()).ok() == Some(version)
        && records
            .iter()
            .zip(1_i64..)
            .all(|((recorded, _), expected)| *recorded == Value::Integer(expected));
    if !complete {
        return Err(unknown());
    }
    // A mark on a version that this build holds has no effect.
    let unusable = records
        .iter()
        .zip(1_i64..)
        .any(|((_, incompatible), v)| v > supported && *incompatible != Value::Integer(0));
    if unusable {
        return Err(StoreError::IncompatibleSchema {
            found: version,
            supported,
        });
    }
    Ok(if version < supported {
        Found::Behind(version)
    } else {
        Found::Ready
    })
}

/// The (version, incompatible) values of the records in version order, or
/// `None` when the file has no record table.
fn read_records(conn: &Connection) -> rusqlite::Result<Option<Vec<(Value, Value)>>> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master
         WHERE type = 'table' AND name = 'schema_migrations')",
        [],
        |r| r.get(0),
    )?;
    if !has_table {
        return Ok(None);
    }
    let mut stmt =
        conn.prepare("SELECT version, incompatible FROM schema_migrations ORDER BY version")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map(Some)
}

/// Apply every pending migration in one transaction: all of them, or none.
pub(crate) fn apply_pending(
    conn: &Connection,
    migrations: &[Migration],
    now_ms: i64,
) -> Result<(), StoreError> {
    // The write lock is taken first and the file is inspected again under it:
    // another open may have migrated it since the first inspection.
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    if let Found::Behind(version) = inspect(&tx, migrations)? {
        for migration in migrations.iter().filter(|m| m.version > version) {
            tx.execute_batch(migration.sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_ms, incompatible)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    migration.version,
                    migration.name,
                    now_ms,
                    migration.incompatible
                ],
            )?;
            tx.pragma_update(None, "user_version", migration.version)?;
        }
    }
    Ok(tx.commit()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        bytes, count, event, leaves_its_log, mode_of, plain, records, schema_of, user_version,
        wal_of, TempDir,
    };
    use crate::{vacuum, Store, StoreError};
    use std::path::Path;

    const NOW_MS: i64 = 1_800_000_000_000;

    const TABLES: [&str; 14] = [
        "meetings",
        "utterances",
        "speakers",
        "summaries",
        "insights",
        "notes",
        "tags",
        "tasks",
        "jobs",
        "models",
        "settings",
        "events",
        "schema_migrations",
        "notes_fts",
    ];

    fn assert_new_schema_v1(s: &Store, case: &str) {
        assert_eq!(s.schema_version().unwrap(), 1, "{case}");
        let found = records(s.conn());
        assert_eq!(found.len(), 1, "{case}: {found:?}");
        let (version, name, applied_ms, incompatible) = &found[0];
        assert_eq!((*version, name.as_str()), (1, "schema_v1"), "{case}");
        assert!(*applied_ms > 0, "{case}: {applied_ms}");
        assert_eq!(*incompatible, 0, "{case}");
        for table in TABLES {
            let found: i64 = s
                .conn()
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name = ?1 AND type = 'table'",
                    params![table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(found, 1, "{case}: missing table {table}");
        }
    }

    /// A version-1 database with one meeting row, closed cleanly.
    fn closed_v1(db: &Path) {
        let s = Store::open(db).unwrap();
        s.insert_meeting("m_old", "kept", 1).unwrap();
    }

    /// Turns a version-1 database into a newer one: one more table, one more
    /// nullable column, and the given `(version, incompatible)` records.
    fn make_newer(conn: &Connection, version: i64, newer: &[(i64, i64)]) {
        conn.execute_batch(
            "CREATE TABLE extra (id TEXT PRIMARY KEY);
             ALTER TABLE meetings ADD COLUMN place TEXT;",
        )
        .unwrap();
        for (record, incompatible) in newer {
            conn.execute(
                "INSERT INTO schema_migrations VALUES (?1, 'later', ?2, ?3)",
                params![record, NOW_MS, incompatible],
            )
            .unwrap();
        }
        conn.pragma_update(None, "user_version", version).unwrap();
    }

    /// A newer, incompatible database whose changes are still in its log, with
    /// no connection open.
    fn incompatible_with_frames_in_its_log(db: &Path) {
        closed_v1(db);
        let conn = leaves_its_log(db);
        make_newer(&conn, 2, &[(2, 1)]);
        drop(conn);
        assert!(!bytes(&wal_of(db)).unwrap().is_empty(), "precondition");
    }

    fn assert_incompatible(result: Result<Store, StoreError>, found: i64) {
        match result {
            Err(StoreError::IncompatibleSchema {
                found: f,
                supported: 1,
            }) => assert_eq!(f, found),
            other => panic!("expected incompatible schema, got {:?}", other.err()),
        }
    }

    #[test]
    fn a_new_or_empty_file_opens_at_version_1_with_one_record() {
        for (empty_file, empty_log) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let case = format!("empty file {empty_file}, empty log {empty_log}");
            let dir = TempDir::new();
            let db = dir.db();
            if empty_file {
                std::fs::write(&db, b"").unwrap();
            }
            if empty_log {
                std::fs::write(wal_of(&db), b"").unwrap();
            }
            let s = Store::open(&db).unwrap_or_else(|e| panic!("{case}: {e}"));
            assert_new_schema_v1(&s, &case);
        }
    }

    #[test]
    fn a_database_with_no_table_at_version_0_is_new() {
        let dir = TempDir::new();
        let db = dir.db();
        {
            let conn = plain(&db);
            conn.execute_batch("CREATE TABLE scratch (x); DROP TABLE scratch;")
                .unwrap();
            assert_eq!(user_version(&conn), 0, "precondition");
            assert!(schema_of(&conn).is_empty(), "precondition");
        }
        assert!(bytes(&db).unwrap().len() > 1, "precondition: a header");

        let s = Store::open(&db).unwrap();

        assert_new_schema_v1(&s, "header only");
    }

    #[test]
    fn a_newer_additive_database_opens_and_stays_as_it_is() {
        let dir = TempDir::new();
        let db = dir.db();
        closed_v1(&db);
        let before = {
            let conn = plain(&db);
            make_newer(&conn, 2, &[(2, 0)]);
            schema_of(&conn)
        };

        let s = Store::open(&db).unwrap();

        let old: String = s
            .conn()
            .query_row("SELECT title FROM meetings WHERE id = 'm_old'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(old, "kept");
        s.insert_meeting("m_new", "written", 2).unwrap();
        s.conn()
            .execute_batch(
                "INSERT INTO utterances (id, meeting_id, source_id, start_ms, end_ms, status,
                     language, revision, text)
                 VALUES ('u1', 'm_new', 'incoming', 0, 1, 'final', 'en', 1, 'line');",
            )
            .unwrap();
        let written = event("e1", "m_new", 1, "line");
        s.journal().append(&written).unwrap();
        assert_eq!(count(s.conn(), "meetings"), 2);
        assert_eq!(count(s.conn(), "utterances"), 1);
        assert_eq!(s.journal().load("m_new").unwrap(), [written]);
        assert_eq!(s.schema_version().unwrap(), 2);
        assert_eq!(schema_of(s.conn()), before);
    }

    #[test]
    fn an_incompatible_database_is_refused_and_unchanged() {
        let dir = TempDir::new();
        let db = dir.db();
        closed_v1(&db);
        {
            let conn = plain(&db);
            // Rollback journal mode: a switch to WAL would change the file.
            conn.pragma_update(None, "journal_mode", "DELETE").unwrap();
            make_newer(&conn, 2, &[(2, 1)]);
        }
        let before = bytes(&db).unwrap();

        assert_incompatible(Store::open(&db), 2);

        assert!(bytes(&db).unwrap() == before, "the database file changed");
        assert!(bytes(&wal_of(&db)).is_none(), "a log appeared");
    }

    #[test]
    fn a_refused_database_with_frames_in_its_log_keeps_its_bytes() {
        let dir = TempDir::new();
        let db = dir.db();
        incompatible_with_frames_in_its_log(&db);
        let main_before = bytes(&db).unwrap();
        let log_before = bytes(&wal_of(&db)).unwrap();

        assert_incompatible(Store::open(&db), 2);

        assert!(
            bytes(&db).unwrap() == main_before,
            "the database file changed"
        );
        assert!(
            bytes(&wal_of(&db)) == Some(log_before),
            "the write-ahead log changed"
        );
    }

    #[test]
    fn every_newer_record_is_checked() {
        let dir = TempDir::new();
        let db = dir.db();
        closed_v1(&db);
        make_newer(&plain(&db), 3, &[(2, 1), (3, 0)]);
        let before = bytes(&db).unwrap();

        assert_incompatible(Store::open(&db), 3);
        assert!(bytes(&db).unwrap() == before, "the database file changed");

        let dir = TempDir::new();
        let db = dir.db();
        closed_v1(&db);
        make_newer(&plain(&db), 3, &[(2, 0), (3, 0)]);
        let s = Store::open(&db).unwrap();
        assert_eq!(s.schema_version().unwrap(), 3);
    }

    #[test]
    fn a_mark_on_a_known_version_does_not_stop_the_open() {
        let dir = TempDir::new();
        let db = dir.db();
        closed_v1(&db);
        let changed = plain(&db)
            .execute("UPDATE schema_migrations SET incompatible = 1", [])
            .unwrap();
        assert_eq!(changed, 1, "precondition");

        let s = Store::open(&db).unwrap();

        assert_eq!(s.schema_version().unwrap(), 1);
    }

    #[test]
    fn a_flag_other_than_zero_is_incompatible() {
        let dir = TempDir::new();
        let db = dir.db();
        closed_v1(&db);
        plain(&db)
            .execute_batch(
                "DROP TABLE schema_migrations;
                 CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT,
                     applied_ms INTEGER, incompatible INTEGER);
                 INSERT INTO schema_migrations VALUES (1, 'schema_v1', 1, 0), (2, 'later', 2, 2);
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        let before = bytes(&db).unwrap();

        assert_incompatible(Store::open(&db), 2);

        assert!(bytes(&db).unwrap() == before, "the database file changed");
    }

    #[test]
    fn missing_or_extra_records_are_an_unknown_schema() {
        let cases: [(&str, bool, &str, i64); 6] = [
            (
                "no record for the version",
                true,
                "PRAGMA user_version = 2",
                2,
            ),
            (
                "a gap in the records",
                true,
                "INSERT INTO schema_migrations VALUES (3, 'later', 1, 0);
                 PRAGMA user_version = 3",
                3,
            ),
            (
                "a record above the version",
                true,
                "INSERT INTO schema_migrations VALUES (2, 'later', 1, 0)",
                1,
            ),
            (
                "tables but no record table",
                true,
                "DROP TABLE schema_migrations",
                1,
            ),
            (
                "version 0 with a table",
                false,
                "CREATE TABLE events (event_id TEXT PRIMARY KEY)",
                0,
            ),
            ("a negative version", true, "PRAGMA user_version = -1", -1),
        ];
        for (case, from_v1, change, expected) in cases {
            let dir = TempDir::new();
            let db = dir.db();
            if from_v1 {
                closed_v1(&db);
            }
            plain(&db).execute_batch(change).unwrap();
            let before = bytes(&db).unwrap();

            let result = Store::open(&db);

            assert!(
                matches!(result, Err(StoreError::UnknownSchema { found }) if found == expected),
                "{case}: {:?}",
                result.err()
            );
            assert!(bytes(&db).unwrap() == before, "{case}: the file changed");
        }
    }

    #[test]
    fn a_file_that_is_not_a_database_is_refused_and_kept() {
        let dir = TempDir::new();
        let db = dir.db();
        let content = vec![b'x'; 4096];
        std::fs::write(&db, &content).unwrap();

        assert!(Store::open(&db).is_err());

        assert!(bytes(&db) == Some(content), "the file changed");
    }

    #[test]
    fn a_failing_migration_leaves_nothing_behind() {
        let broken = [
            MIGRATIONS[0],
            Migration {
                version: 2,
                name: "broken",
                incompatible: false,
                sql: "CREATE TABLE half (x); INSERT INTO missing_table VALUES (1);",
            },
        ];

        // (a) a new database.
        let dir = TempDir::new();
        let db = dir.db();
        assert!(Store::open_with(&db, &broken, NOW_MS, vacuum).is_err());
        {
            let conn = plain(&db);
            assert_eq!(user_version(&conn), 0);
            assert!(schema_of(&conn).is_empty(), "{:?}", schema_of(&conn));
        }

        // (b) a version-1 database whose row is still in the log.
        let dir = TempDir::new();
        let db = dir.db();
        drop(Store::open(&db).unwrap());
        leaves_its_log(&db)
            .execute(
                "INSERT INTO meetings VALUES ('m1', 'in the log', 1, NULL)",
                [],
            )
            .unwrap();
        assert!(!bytes(&wal_of(&db)).unwrap().is_empty(), "precondition");
        let main_before = bytes(&db).unwrap();

        assert!(Store::open_with(&db, &broken, NOW_MS, vacuum).is_err());

        assert!(
            bytes(&db).unwrap() == main_before,
            "the database file changed"
        );
        let conn = plain(&db);
        assert_eq!(user_version(&conn), 1);
        assert!(!schema_of(&conn).iter().any(|o| o.contains("half")));
        assert_eq!(records(&conn).len(), 1);
        assert_eq!(count(&conn, "meetings"), 1);
    }

    #[test]
    fn a_pending_migration_is_applied_once() {
        let list = [
            MIGRATIONS[0],
            Migration {
                version: 2,
                name: "extra",
                incompatible: true,
                sql: "CREATE TABLE extra (id TEXT PRIMARY KEY);",
            },
        ];
        let dir = TempDir::new();
        let db = dir.db();
        closed_v1(&db);

        let first = Store::open_with(&db, &list, NOW_MS, vacuum).unwrap();
        assert_eq!(first.schema_version().unwrap(), 2);
        assert_eq!(
            records(first.conn())[1],
            (2, "extra".to_string(), NOW_MS, 1)
        );
        assert_eq!(count(first.conn(), "meetings"), 1);
        let objects = schema_of(first.conn());
        drop(first);

        let second = Store::open_with(&db, &list, NOW_MS + 1, vacuum).unwrap();
        assert_eq!(records(second.conn()).len(), 2);
        assert_eq!(records(second.conn())[1].2, NOW_MS);
        assert_eq!(schema_of(second.conn()), objects);
    }

    #[test]
    fn a_refused_file_and_its_log_are_still_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let db = dir.db();
        incompatible_with_frames_in_its_log(&db);
        for file in [db.clone(), wal_of(&db)] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        assert_incompatible(Store::open(&db), 2);

        assert_eq!(mode_of(&db), 0o600, "database file");
        assert_eq!(mode_of(&wal_of(&db)), 0o600, "-wal file");
    }

    #[test]
    fn an_accepted_open_empties_the_log() {
        for with_reader in [false, true] {
            let dir = TempDir::new();
            let db = dir.db();
            closed_v1(&db);
            leaves_its_log(&db)
                .execute(
                    "INSERT INTO meetings VALUES ('m1', 'in the log', 1, NULL)",
                    [],
                )
                .unwrap();
            assert!(!bytes(&wal_of(&db)).unwrap().is_empty(), "precondition");
            let reader = with_reader.then(|| {
                let reader = plain(&db);
                reader
                    .execute_batch("BEGIN; SELECT count(*) FROM meetings;")
                    .unwrap();
                reader
            });

            let s = Store::open(&db).unwrap_or_else(|e| panic!("reader {with_reader}: {e}"));

            assert_eq!(count(s.conn(), "meetings"), 2, "reader {with_reader}");
            if reader.is_none() {
                assert_eq!(bytes(&wal_of(&db)).unwrap().len(), 0);
            }
        }
    }

    #[test]
    fn an_empty_or_missing_file_with_a_log_is_refused_and_kept() {
        use std::os::unix::fs::PermissionsExt;
        let committed_rows = {
            let dir = TempDir::new();
            let db = dir.db();
            closed_v1(&db);
            leaves_its_log(&db)
                .execute(
                    "INSERT INTO meetings VALUES ('m1', 'in the log', 1, NULL)",
                    [],
                )
                .unwrap();
            bytes(&wal_of(&db)).unwrap()
        };
        assert!(committed_rows.len() > 1, "precondition");
        // Name, content of the database file (None: no file), content of the log.
        type Case<'a> = (&'a str, Option<&'a [u8]>, &'a [u8]);
        let cases: [Case; 4] = [
            ("empty file", Some(b""), b"w"),
            ("1 byte file", Some(b"x"), b"w"),
            ("no file", None, b"w"),
            ("empty file, committed rows", Some(b""), &committed_rows),
        ];
        for (case, main, log) in cases {
            let dir = TempDir::new();
            let db = dir.db();
            let mut files = vec![wal_of(&db)];
            std::fs::write(wal_of(&db), log).unwrap();
            if let Some(content) = main {
                std::fs::write(&db, content).unwrap();
                files.push(db.clone());
            }
            for file in &files {
                std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644)).unwrap();
            }

            let result = Store::open(&db);

            assert!(
                matches!(result, Err(StoreError::UnknownSchema { found: 0 })),
                "{case}: {:?}",
                result.err()
            );
            assert!(bytes(&wal_of(&db)).as_deref() == Some(log), "{case}: log");
            assert!(bytes(&db).as_deref() == main, "{case}: database file");
            for file in &files {
                assert_eq!(mode_of(file), 0o600, "{case}: {}", file.display());
            }
        }
    }

    #[test]
    fn opens_at_the_same_moment_all_succeed() {
        const OPENS: usize = 4;
        for round in 0..100 {
            let dir = TempDir::new();
            let db = dir.db();
            let start = std::sync::Barrier::new(OPENS);
            std::thread::scope(|scope| {
                let opens: Vec<_> = (0..OPENS)
                    .map(|_| {
                        scope.spawn(|| {
                            start.wait();
                            Store::open(&db)
                        })
                    })
                    .collect();
                for open in opens {
                    let result = open.join().unwrap();
                    assert!(result.is_ok(), "round {round}: {:?}", result.err());
                }
            });
            assert_eq!(records(&plain(&db)).len(), 1, "round {round}");
        }
    }
}
