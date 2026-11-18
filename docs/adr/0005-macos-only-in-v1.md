# 0005. macOS only in v1

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-41

## Context

Capturing a meeting app's audio needs deep operating system support. On macOS
a Core Audio process tap (macOS 14.4 or later) can capture one app's audio.
Each platform needs its own capture, virtual device and permission work. A
small team cannot build and test all of them at once.

## Decision

v1 targets macOS 14.4 or later on Apple silicon only. Windows and Linux come
after v1.

## Consequences

- One audio path to build, measure and test. CI runs on a macOS runner.
- Intel Macs and older macOS versions are not supported.
- Platform code stays behind the traits in `crates/audio-io`, so a later port
  adds an implementation and does not rewrite the core.
- The README and the platform table in `docs/tech-stack.md` state the floor.
