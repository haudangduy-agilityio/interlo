# 0023. One database file: schema version rule, journal and delete cleanup

- Status: accepted
- Date: 2026-11-26
- Decision ID: D-193

## Context

D-134 sets one SQLite file with one schema version that the `store` crate owns.
D-125 lets an older build open a newer database when every newer migration is
additive. The code before this change did not follow either: the journal opened
its own connection, set the version itself and refused every higher version. A
design challenge on 2026-11-26 also found that words of deleted text could stay
on disk, that an exit during a delete was never cleaned up, and that some
statement forms could rewrite a final line.

This record replaces item 3 of ADR 0013, the version sentence of ADR 0019, and
in ADR 0008 the separate journal delete call and the checkpoint as the whole
cleanup of a delete.

## Decision

### One file, one owner

All tables, the journal included, are in one database file. `crates/store` owns
the file, the schema version (`PRAGMA user_version`), the migrations and the
journal. The journal uses the connection of the store.

### Migrations

A migration is a numbered SQL file in `crates/store/migrations/`. The open call
applies every pending migration in one transaction: all of them, or none. Each
applied migration adds one record to the table `schema_migrations` (version,
name, UTC milliseconds, incompatible flag). Migration 1 is not edited after
this change; every later schema change is a new migration. This is stricter than
item 1 of ADR 0013.

### Version rule

The check runs before any statement that writes.

| File | Result |
|---|---|
| Version 0 with no object; no file or an empty file with no log data beside it | New database: apply every migration |
| Version 0 with an object, or a negative version | Refused: unknown schema |
| Records do not list exactly the versions 1 to the schema version, each once | Refused: unknown schema |
| A record above the supported version whose incompatible flag is not 0 | Refused: incompatible schema |
| Version below the supported version | Apply the pending migrations |
| Version equal to or above the supported version | Open without a schema change |
| Empty or missing database file with a log of 1 byte or more beside it | Refused: unknown schema |

A refused open changes no content of the database file or its log. The file
mode is still set to 0600.

### Additive migration

A migration is additive only when all of these are true. Any other migration is
recorded as incompatible.

- It adds only tables, indexes, or columns that allow NULL or have a default.
- It adds no trigger or constraint that makes a statement of an older build
  fail.
- A new table that holds meeting text deletes its rows when the meeting row is
  deleted.
- After it, the meeting delete of a build that does not know it still removes
  all text of that meeting and can leave no copy of it. So it adds no column for
  meeting text on a row that survives the delete, no table keyed by a session
  id, and no search index without secure delete.

### Delete and cleanup

- Deleting a meeting removes its rows and its journal rows in one transaction.
  The session id of a journal row is the meeting id. The journal has no delete
  call of its own.
- The same transaction writes a pending mark, one row of the `settings` table.
- After the commit the database file is rebuilt (`VACUUM`) and the log is
  truncated. The result says scrubbed only when both worked, and only then is
  the mark removed.
- Every accepted open truncates the log. When the mark is there, the open
  rebuilds the file first. A cleanup that cannot finish stays pending and does
  not stop the open.
- The search index of notes uses its secure delete option.

### Guards on raw statements

Triggers abort a raw statement that changes or removes a final line or a journal
row. The tables `utterances` and `events` have no row number (`WITHOUT ROWID`),
and the id of a line cannot change.

## Consequences

- A build never writes into a database that it refuses.
- A build that opens a newer additive database works on the tables it knows.
- A database of an older development build is refused and kept as it is.
- A meeting delete takes time in proportion to the size of the file, because the
  file is rebuilt.
- The truncated log releases disk blocks but does not overwrite them.
- `Store::conn()` still reaches tables that have no guard until it is private
  (ADR 0013, task E1-01).
- Not covered: loss of the last committed rows after a power cut, and two app
  processes that open the file at the same moment.
