# 0015. Automatic updates: signed, outside meetings, from GitHub Releases

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-61

## Context

D-58 said v1 had manual updates only. The owner reversed this for v1 (D-61).
An updater adds a second network use, so it needs rules that keep the privacy
promise: nothing leaves the Mac during a meeting, and nothing about the user is
sent at any time.

## Decision

- **When it checks.** At app start and every 24 hours. Never while a meeting
  is running. The "Check for updates" button in About (Settings) checks at once,
  and it also never runs during a meeting.
- **How it installs.** The app downloads the update in the background, verifies
  the signature, then shows "Restart to update". The app never restarts while a
  meeting runs.
- **Switch.** Settings has "Update automatically", on by default. With the
  switch off, the app only tells the owner that a new version exists and
  downloads nothing.
- **Source.** GitHub Releases of the Interlo repository. Each release carries
  an updater signature made with its own key. That key is separate from the
  Apple Developer ID certificate (ADR 0004). The release is also notarized.
  An update with a wrong signature is refused.
- **Policy gate.** The update host is on the policy gate allow list
  (`crates/meeting-core/src/policy.rs`). The check and the download go through the
  policy-owned client (ADR 0007) and nowhere else.
- **Sent data.** Only the request for the manifest file. No machine id, no
  user id, no usage data. The server sees an IP address, as for any web request.
  The README says so.
- **Egress rule.** Network only for a model download or a driver installer
  download the owner starts, and the update check outside meetings (guardrail 1,
  ADR 0016).
- The update half of D-58 is superseded by this ADR. D-58 stays valid for
  diagnostics: nothing is sent automatically, and "Export diagnostics" writes a
  file. The "no automatic update" wording of D-40 and ADR 0004 is replaced by
  this ADR; their signing and notarization rules stay.

## Consequences

- The privacy text gains one sentence: the network is also used for the update
  check outside meetings. The wireframe strings for the disclosure stay as they
  are.
- The updater signing key is a new secret. It lives in the macOS Keychain, never
  in the repository or in CI (owner task H-21). Losing it means users cannot get
  signed updates.
- ADR 0007 part 2 (no outbound entitlement in the main app) must leave room for
  the update host before M4.
- Built at M4 with signing and notarization (M4-15, X-10).
- Tests: an update with a wrong signature is refused; no update download starts
  while a meeting runs (`docs/testing.md`).
