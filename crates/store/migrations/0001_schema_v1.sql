CREATE TABLE meetings (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    started_ms  INTEGER NOT NULL,
    ended_ms    INTEGER
);

CREATE TABLE utterances (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    source_id   TEXT NOT NULL,
    speaker_id  TEXT,
    start_ms    INTEGER NOT NULL,
    end_ms      INTEGER NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('partial', 'stable', 'final')),
    language    TEXT NOT NULL CHECK (language IN ('en', 'vi')),
    revision    INTEGER NOT NULL DEFAULT 0,
    text        TEXT NOT NULL
) WITHOUT ROWID;
CREATE INDEX utterances_by_meeting ON utterances(meeting_id, start_ms);

-- The id of a line never changes: UPDATE OR REPLACE on an id would remove the
-- line that holds it without firing a delete trigger.
CREATE TRIGGER utterances_id_fixed
BEFORE UPDATE ON utterances
WHEN NEW.id IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'line id is fixed');
END;

CREATE TRIGGER utterances_final_guard
BEFORE UPDATE ON utterances
WHEN OLD.status = 'final'
BEGIN
    SELECT RAISE(ABORT, 'final utterance is immutable');
END;

-- INSERT OR REPLACE and REPLACE delete the old row without firing delete
-- triggers (recursive_triggers is off), so the insert is guarded as well.
CREATE TRIGGER utterances_final_no_replace
BEFORE INSERT ON utterances
WHEN EXISTS (SELECT 1 FROM utterances WHERE id = NEW.id AND status = 'final')
BEGIN
    SELECT RAISE(ABORT, 'final utterance is immutable');
END;

-- store_delete_open() is a function that Store::init registers on each
-- connection. It returns 1 only inside Store::delete_meeting. A connection
-- without the function cannot delete a final line at all.
CREATE TRIGGER utterances_final_no_delete
BEFORE DELETE ON utterances
WHEN OLD.status = 'final' AND store_delete_open() = 0
BEGIN
    SELECT RAISE(ABORT, 'final utterance is immutable');
END;

CREATE TABLE speakers (
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    label       TEXT NOT NULL,
    PRIMARY KEY (meeting_id, id)
);

CREATE TABLE summaries (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    body        TEXT NOT NULL,
    created_ms  INTEGER NOT NULL
);

CREATE TABLE insights (
    id                 TEXT PRIMARY KEY,
    meeting_id         TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    kind               TEXT NOT NULL,
    body               TEXT NOT NULL,
    evidence_line_ids  TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE tags (
    name        TEXT PRIMARY KEY
);

-- seq is the rowid alias. notes_fts is keyed by it, so it must stay stable
-- (a table without an INTEGER PRIMARY KEY may renumber its rowids on VACUUM).
CREATE TABLE notes (
    seq         INTEGER PRIMARY KEY,
    id          TEXT NOT NULL UNIQUE,
    meeting_id  TEXT REFERENCES meetings(id) ON DELETE SET NULL,
    text        TEXT NOT NULL,
    tag         TEXT REFERENCES tags(name) ON UPDATE CASCADE ON DELETE SET NULL,
    created_ms  INTEGER NOT NULL
);

CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT REFERENCES meetings(id) ON DELETE SET NULL,
    title       TEXT NOT NULL,
    due_ms      INTEGER,
    status      TEXT NOT NULL CHECK (status IN ('open', 'done'))
);

CREATE TABLE jobs (
    id              TEXT PRIMARY KEY,
    meeting_id      TEXT,
    kind            TEXT NOT NULL,
    state           TEXT NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    run_at_ms       INTEGER NOT NULL,
    lease_until_ms  INTEGER,
    payload         TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE models (
    id           TEXT PRIMARY KEY,
    url          TEXT NOT NULL,
    sha256_hex   TEXT NOT NULL,
    size_bytes   INTEGER,
    verified_ms  INTEGER
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

CREATE INDEX summaries_by_meeting ON summaries(meeting_id);
CREATE INDEX insights_by_meeting ON insights(meeting_id);
CREATE INDEX notes_by_meeting ON notes(meeting_id);
CREATE INDEX tasks_by_meeting ON tasks(meeting_id);
CREATE INDEX jobs_by_meeting ON jobs(meeting_id);
CREATE INDEX jobs_by_state_run_at ON jobs(state, run_at_ms);

-- The index row has the same rowid as notes.seq, so a note delete or update
-- finds its index row by key (O(log n)) and never scans the table.
CREATE VIRTUAL TABLE notes_fts USING fts5(text);
-- Without this option the index keeps the words of a deleted note.
INSERT INTO notes_fts(notes_fts, rank) VALUES ('secure-delete', 1);

CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, text) VALUES (NEW.seq, NEW.text);
END;
CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
    DELETE FROM notes_fts WHERE rowid = OLD.seq;
END;
CREATE TRIGGER notes_fts_update AFTER UPDATE OF text ON notes BEGIN
    DELETE FROM notes_fts WHERE rowid = OLD.seq;
    INSERT INTO notes_fts(rowid, text) VALUES (NEW.seq, NEW.text);
END;

-- One row per applied migration. An older build reads this table to decide
-- whether it can open a newer database (D-125).
CREATE TABLE schema_migrations (
    version       INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    applied_ms    INTEGER NOT NULL,
    incompatible  INTEGER NOT NULL CHECK (incompatible IN (0, 1))
);

-- The append-only event journal. session_id is the meeting id.
CREATE TABLE events (
    event_id        TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    sequence        INTEGER NOT NULL,
    schema_version  TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    occurred_at     TEXT NOT NULL,
    payload         TEXT NOT NULL,
    UNIQUE (session_id, sequence)
) WITHOUT ROWID;

CREATE TRIGGER events_no_update
BEFORE UPDATE ON events
BEGIN
    SELECT RAISE(ABORT, 'journal is append-only');
END;

-- INSERT OR REPLACE deletes the old row without firing delete triggers.
CREATE TRIGGER events_no_replace
BEFORE INSERT ON events
WHEN EXISTS (SELECT 1 FROM events WHERE event_id = NEW.event_id)
  OR EXISTS (SELECT 1 FROM events WHERE session_id = NEW.session_id AND sequence = NEW.sequence)
BEGIN
    SELECT RAISE(ABORT, 'journal is append-only');
END;

CREATE TRIGGER events_no_delete
BEFORE DELETE ON events
WHEN store_delete_open() = 0
BEGIN
    SELECT RAISE(ABORT, 'journal is append-only');
END;
