# 0018. The updater never installs a lower version

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-79

## Context

Every signed release stays downloadable on the release host. If the updater
accepted any validly signed version, an attacker could replay an old manifest or
an old release with known flaws (Threat Model T-02). A bad release also needs a
way out that does not weaken this rule.

## Decision

The updater installs a version only when it is higher than the running version.
A manifest carries a version and an expiry, and an expired manifest is refused
(task X-13). A bad release is withdrawn from the release host and replaced by a
higher fixed version. The only way back for a user is a manual reinstall.

## Consequences

Replay of an old release is refused. A rollback is always a new, higher version,
so each fix goes through the full release path. The check is a test
(`updater_refuses_older_and_expired_manifest`, proposed in X-13). The release
pull plan lives in the former Operations Plan (dropped, D-169).
