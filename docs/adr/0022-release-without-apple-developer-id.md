# 0022. Release without an Apple Developer account: a locally signed DMG and an install guide

- Status: accepted
- Date: 2026-11-19
- Decision ID: D-188
- Supersedes: ADR 0004 (D-40)

## Context

The owner has no Apple Developer Program account. Without it there is no Developer ID
certificate and no notarization. Interlo is not published in an app store. The owner
gives people a DMG file and a guide to install it.

## Decision

- A release is built on the owner's Mac, signed with the owner's local self-made
  certificate, and packaged as a DMG with a SHA-256 checksum. It is not notarized.
- There is no Developer ID certificate, no App Store release and no TestFlight.
- CI builds and tests only. No signing key is in the repository or in CI (unchanged).
- The README has an install guide. It tells the user to expect a macOS warning on the
  first open. It shows the steps to allow the app in System Settings, Privacy & Security.
- The updater (ADR 0015) keeps its own signature key (owner task H-21). It does not
  depend on any Apple account.

## Consequences

- Every user sees a macOS warning on the first open. On macOS 15 and later the user
  allows the app in System Settings, Privacy & Security, with "Open Anyway". The goal of
  ADR 0004, no Gatekeeper warning, no longer holds.
- Microphone and audio-capture permissions are tied to the signing identity. They stay
  after an update only when every release is signed with the same certificate. Losing the
  certificate means every user grants them again. Keep a backup (owner task H-10).
- Not verified yet: the exact steps on each macOS version, and whether an update that the
  app installs shows the warning again. The M4 smoke test and the updater test (M4-20)
  check both on a Mac that did not build the app.
- The owner drops tasks H-12 (Apple enrolment) and H-17 (notarytool credentials). The
  release script and its checks have no notarization step.
- If an Apple account exists later, notarization can return with a new ADR that
  supersedes this one.
