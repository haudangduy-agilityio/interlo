# 0004. Developer ID signing and notarization for releases

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-40

## Context

macOS blocks or warns on software that is not signed and notarized. Interlo
needs microphone and audio capture permissions, which depend on a stable app
identity. CI must not hold signing secrets.

## Decision

Release builds are signed with a Developer ID certificate and notarized.
Local builds use a self-made certificate. CI builds and tests only: no
signing, no secrets and no deployment in the workflow. Updates: see ADR 0015,
which replaces the earlier "no automatic update" rule.

## Consequences

- Users can open a release without a Gatekeeper warning, and permission grants
  stay attached to the app.
- Signing and notarization run on the owner's machine at release time, as the
  last station of `docs/workflow.md`. The owner names the tag.
- Users get updates through the signed updater of ADR 0015 (D-61).
- A signing key is never part of the repository or of CI.
