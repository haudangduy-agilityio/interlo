# 0008. Deleting a meeting deletes its journal rows and uses secure delete

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-46
- Update 2026-11-26: [ADR 0023](0023-one-database-version-rule.md) (D-193)
  replaces the separate journal delete call and the checkpoint as the whole
  cleanup of a delete.

## Context

The store deletes a meeting and its utterances, speakers, summaries and
insights by cascade. The journal is a separate table with the raw event text
of each session. If the journal rows stay, the transcript text stays on disk
after the owner deleted the meeting, which breaks the privacy promise. SQLite
also leaves deleted pages readable in the file unless it overwrites them.

## Decision

Deleting a meeting also deletes its journal rows, through
`Journal::delete_session`, in the same user action as the store delete. The
database sets `PRAGMA secure_delete`, so removed content is overwritten.

The same action also removes what the meeting owns (D-46):

- Notes created in the meeting (notes with a `meeting_id`) are deleted with it.
- Daily notes that are not tied to a meeting stay.
- Confirmed tasks stay, with the meeting link cleared. They are the owner's
  to-do items. Unconfirmed task drafts from that meeting are deleted.
- Task rule: owner to confirm (H-19). The assistant added it as an assumption.

## Consequences

- After a delete, no transcript text of that meeting remains in the store or
  the journal file.
- The journal is append-only except for this one session-level delete. Replay
  tests must cover a deleted session.
- Secure delete costs some write time and does not clean copies in backups or
  the write-ahead log until a checkpoint. Treat the checkpoint as part of the
  delete.
- A test must show that the journal returns no rows for a deleted session.

- Implementation status (2026-11-26, D-193): `Store::delete_meeting` removes the
  meeting rows and the journal rows in one transaction, then rebuilds the file,
  truncates the log and reports `scrubbed`. A reader that holds the database open
  makes `scrubbed` false; `Store::scrub` and the next open try again. A test reads
  the database and log bytes after the delete.
