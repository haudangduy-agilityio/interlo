# 0002. Local only, no cloud in v1

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-42

## Context

Meetings carry private speech. Users must be able to trust that Interlo does
not send that speech anywhere. A cloud engine would also add accounts, billing,
a backend and a larger attack surface.

## Decision

In v1 there is no cloud engine, no account, no sharing and no backend. Speech
recognition, translation, voices and summaries run on the Mac. The only
network use is a model download or a driver installer download that the owner
starts, and the update check outside meetings (ADR 0015, ADR 0016). Every
outbound call goes through the policy gate in `crates/meeting-core/src/policy.rs`. There is
no local HTTP server (A-13).

## Consequences

- The privacy promise is simple and testable: nothing leaves the Mac during a
  meeting (G6).
- Quality and speed are bounded by what runs on Apple silicon. The latency gate
  (D-33) measures this with live speech.
- No cloud fallback exists when a local model fails. A failed segment keeps its
  caption and plays nothing (FR-66).
- Adding a cloud engine later needs a new decision that supersedes this one,
  and a change to the policy gate.
