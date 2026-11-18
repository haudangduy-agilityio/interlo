# Security policy

## Report a vulnerability

Report it privately. Do not open a public issue or a public merge request.

On GitLab, open a **confidential issue**. On GitHub, use the "Report a
vulnerability" button on the Security tab (private vulnerability reporting is
turned on when the repository moves to GitHub, task H-20). The project does not
publish an email address: a report goes through the host's private channel, so the
report and its replies stay in one place.

Include the version or commit, the steps to reproduce, and the impact. We aim
to confirm a report within 7 days.

## Scope

Interlo promises that nothing leaves the Mac during a meeting. These areas are
in scope:

- **Policy gate.** Any way to make an outbound call that does not pass the gate
  in `crates/meeting-core/src/policy.rs`.
- **IPC.** Any command or event between the Rust core and the UI that exposes
  data or a capability it should not, or accepts input it does not validate.
- **Downloads.** Any way to run a model or file that failed its checksum, or to
  start a download without the owner's click.
- **Quick actions.** Reading a password field, logging keystrokes, or pasting
  without the owner's confirm.

Also in scope: leaks of secrets (they live in the macOS Keychain only), stored
audio, and transcript text in logs while debug logs are off.

## Update signature and key handling

Updates are downloaded from the GitHub Releases of this repository and are
accepted only when their signature is valid. The updater uses its own signing
key, separate from the Apple Developer ID certificate. The key is stored in the
macOS Keychain of the maintainer who signs releases. It is never in the
repository or in CI. An update with a wrong signature is refused. A way to
install an update that failed this check, or to run an update check during a
meeting, is in scope.

## Out of scope

- Attacks that need a modified or already compromised macOS.
- Vulnerabilities in a third-party model or in a dependency with no way to
  reach it from Interlo.
- Issues in a development build or a build you made yourself with changed
  code.

## Supported versions

Interlo is pre-alpha. Only the latest commit on the default branch is
supported.
