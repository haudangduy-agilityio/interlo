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

This record replaces items 1 and 3 of ADR 0013, the version sentence of ADR
0019, and in ADR 0008 the separate journal delete call and the checkpoint as the
whole cleanup of a delete.

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
this change; every later schema change is a new migration. This replaces item 1
of ADR 0013, which let schema v1 change until the first public tag.

### Version rule

The check runs before any statement that writes.

| File | Result |
|---|---|
| Version 0 with no object; no file, or a file of at most 1 byte with no log data beside it | New database: apply every migration |
| Version 0 with an object, or a negative version | Refused: unknown schema |
| Records do not list exactly the versions 1 to the schema version, each once | Refused: unknown schema |
| A record above the supported version whose incompatible flag is not 0 | Refused: incompatible schema |
| Version below the supported version | Apply the pending migrations |
| Version equal to or above the supported version | Open without a schema change |
| A missing database file, or one of at most 1 byte, with a log of 1 byte or more beside it | Refused: unknown schema |

A refused open changes no content of the database file or its log. The file
mode is still set to 0600.

A symbolic link as the database path stands for its target: every rule above
applies to the target. A link into a missing folder, a chain of 32 links and
anything at the path that is not a file are errors, and the open creates and
changes nothing for them.

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
  id, and no new search index over meeting text: the cleanup rebuilds only the
  search index of notes.

### Delete and cleanup

- Deleting a meeting removes its rows and its journal rows in one transaction.
  The session id of a journal row is the meeting id. The journal has no delete
  call of its own.
- The same transaction writes a pending mark, one row of the `settings` table.
- After the commit the search index of notes and the database file (`VACUUM`)
  are rebuilt and the log is truncated. The result says scrubbed only when all of
  this worked, and only then is the mark removed.
- Every accepted open truncates the log. When the mark is there, the open
  rebuilds both first. A cleanup that cannot finish stays pending and does not
  stop the open.
- The search index of notes uses its secure delete option. The option keeps the
  page keys of the index, which hold the start of words, so the cleanup rebuilds
  the index.

### Guards on raw statements

Triggers abort a raw statement that changes or removes a final line or a journal
row. The tables `utterances` and `events` have no row number (`WITHOUT ROWID`),
and the id of a line cannot change.

## Consequences

- A build never writes into a database that it refuses.
- A build that opens a newer additive database works on the tables it knows.
- A database of an older development build is refused and kept as it is.
- A meeting delete takes time in proportion to the size of the data, because the
  search index and the file are rebuilt. The store connection is busy until the
  end, so journal appends wait.
- The file rebuild needs free disk space, up to twice the size of the file.
  Without it the cleanup stays pending.
- The truncated log releases disk blocks but does not overwrite them.
- `Store::conn()` still reaches tables that have no guard until it is private
  (ADR 0013, task E1-01).
- Not covered:
  - loss of the last committed rows after a power cut;
  - two app processes that open the file at the same moment;
  - the page keys that a delete or an edit of one note leaves in the search index
    until the next cleanup;
  - temporary files that the database library may write outside the database
    during a delete or a rebuild;
  - tag names that only deleted notes used.
