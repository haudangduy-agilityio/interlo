# 0013. Database and time rules

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-59
- Update 2026-11-26: [ADR 0023](0023-one-database-version-rule.md) (D-193)
  replaces items 1 and 3 and the refusal of every higher version in the
  consequences. Every schema change after that record is a new migration.

## Context

The journal and the store keep user data in SQLite. Without rules, an old
database can be opened by a newer or older build, a schema change can strand
early users, and times can mix zones.

## Decision

1. Schema v1 may change freely until the first public tag. From the first
   public tag, every schema change needs a migration.
2. `Store::conn()` becomes crate-private before E1 (task E1-01 includes it).
3. The journal sets `PRAGMA user_version` now, at version 1. Opening a database
   with a higher version is refused.
4. Dates and times are stored as UTC milliseconds. They are shown and exported
   in the user's local time zone at the moment of use.

## Consequences

- An old build never writes into a database from a newer build.
- Before the first public tag, a schema change may need the owner to delete the
  local database; after it, a migration with a test is required.
- No other crate can run raw SQL on the store once `Store::conn()` is
  crate-private; they call store functions.
- A time value in the database is a number, not text. A change of time zone or
  daylight saving changes only what the user sees.
- Tests need a database with a higher `user_version` (refused) and a
  round-trip of a UTC millisecond value shown in two zones.
