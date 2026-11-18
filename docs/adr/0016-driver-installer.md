# 0016. Driver installer: one press, pinned and verified, never bundled

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-69

## Context

Interlo needs a virtual audio device to carry the owner's English voice into the
meeting app. D-10 said the install is guided by steps and the driver is not
bundled. The owner changed this (D-69): the app installs by itself, and the user
only presses Install. The driver licence does not allow us to redistribute the
installer, so the app must fetch it from the driver's own public release.

## Decision

Pressing Install at Setup does the whole job:

1. The Rust shell shows the native confirmation with name, source and size, as
   for a model download (D-52, ADR 0006).
2. The shell downloads the official installer from the driver's own public
   release. The URL is pinned and the SHA-256 is in a manifest in the
   repository. The shell never takes a URL from the webview. A hash mismatch
   is refused and nothing runs.
3. The shell runs the installer with the macOS administrator prompt. The app
   never sees the password, and the prompt cannot be skipped.
4. The app waits up to 15 s for the device to appear.
5. The app plays a loopback test phrase and runs the setup check again.
6. If macOS needs an audio service restart or a restart of the Mac, the app
   says so in plain words. The user runs no manual Terminal steps.

The installer is never bundled in the app and never redistributed. The driver
host is on the policy gate allow list. M0-18 still decides the licence terms for
download on click (done check: the terms URL and the decision "download on
click, never bundle").

D-10 is superseded for the install flow: its "guided steps" are replaced by the
one-press install. D-10 keeps "not bundled". H-02 (the
owner installs the devices by hand from the private guide) stays only until the
in-app installer exists.

## Consequences

- The egress rule becomes: network only for a model download or a driver
  installer download the owner starts, and the update check outside meetings
  (guardrail 1, ADR 0015).
- The administrator prompt always shows.
- A failed step ends in a plain message that names the step (download,
  verify, install, device wait, loopback test).
- Work item in M1, built on the M1-01 and M1-42 pattern. Tests use a fake
  download, a fake hash and a fake installer; the admin prompt itself is checked
  by hand once per release.
- Tests: an installer whose hash does not match is refused
  (`docs/testing.md`).
