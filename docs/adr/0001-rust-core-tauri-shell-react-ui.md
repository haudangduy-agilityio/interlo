# 0001. Rust core with a Tauri 2 shell and a React UI

- Status: accepted
- Date: 2026-11-17

## Context

Interlo captures and plays audio in real time, runs speech and translation
models on the Mac, and shows captions and a daily workspace. The audio path has
hard timing limits: callbacks must not allocate, lock or log. The interface
needs fast iteration and a large component ecosystem. The app must start with a
small memory footprint.

## Decision

Put all logic in a Rust workspace of small crates (`crates/`). Use Tauri 2 as
the app shell (`apps/desktop/src-tauri`) for the window, commands and event
channels. Build the interface in React 19 with TypeScript and Vite
(`apps/ui`). The UI talks to the core only through the typed IPC contract in
`docs/ipc-contract.md`.

## Consequences

- Real-time audio code stays in Rust, free of a garbage collector and of the
  webview.
- The shell uses the system web view, so the bundle is small, but UI behaviour
  depends on the macOS web view version.
- Two languages need two toolchains and two test runners. `scripts/check.sh`
  runs both, and a contract fixture test keeps the Rust and TypeScript types in
  step.
- React stays (D-191, 2026-11-20). The earlier plan to compare idle memory
  with Svelte at M0b is dropped for v1.
