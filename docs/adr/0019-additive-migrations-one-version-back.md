# 0019. Database migrations are additive and the previous version can read them

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-80
- Update 2026-11-26: the store no longer refuses every higher version. The
  version rule and the meaning of "additive" are in
  [ADR 0023](0023-one-database-version-rule.md) (D-193).

## Context

The store refuses a database with a higher schema version than it knows
(ADR 0013). The updater never installs a lower version (ADR 0018), but a user
may reinstall the previous version by hand after a bad release. Whatever the
first shared build writes is what version N-1 must read for ever.

## Decision

Migrations only add (new tables, new nullable columns). They never drop or
rewrite data that version N-1 reads. Version N-1 must open a database written
by version N. This rule is fixed before the first shared build: a preview build
given to a colleague counts as the first shared build. A copy of the database is
made before every migration (the former Operations Plan, dropped by D-169: backup).

## Consequences

A manual reinstall of the previous version keeps the owner's meetings. Some
schema clean-ups wait for a major version. A test opens a database written by
version N with the code of version N-1 (proposed in the former Operations Plan (dropped, D-169) tasks).
