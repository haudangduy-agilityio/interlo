# Tech stack

Status values: **Decided** (do not change without a new decision), **Rule set**
(the choice follows a measured rule), **Later** (added in the task that needs it).

## Platform

| Item | Value | Status |
|---|---|---|
| Operating system | macOS 14.4 or later (Core Audio process tap needs 14.4) | Decided (D-41) |
| Hardware | Apple silicon only in v1 | Decided (D-41) |
| Windows, Linux | After v1 (phase P6) | Decided (D-41) |
| Distribution | A DMG signed with a self-made certificate, not notarized, no store; an install guide in the README; signed automatic updates from GitHub Releases | Decided (ADR 0022, D-61) |
| Licence | MIT for the code; models are not bundled | Decided (D-39) |

## Languages and tools

| Item | Version on the owner's Mac (2026-11-16) | Notes |
|---|---|---|
| Rust | 1.99.1 (edition 2021) | Workspace in the repo root |
| Node | 26.5.0, npm 11.17.0 | UI build only |
| Python | 3.14.7 | Measurement scripts only, never shipped |

## Rust crates in use

| Crate | Used by | Why |
|---|---|---|
| serde, serde_json | the crates that need them (not engine-host, model-store, playback, quick-actions or store) | Event and IPC serialization |
| thiserror | all | Error enums |
| rusqlite (bundled) | meeting-core, store | SQLite with WAL, journal, projections, full-text search |
| tauri 2, tauri-build 2 | apps/desktop | App shell, commands, event channels |

## Rust crates added later (Later)

| Crate | Task | Why |
|---|---|---|
| cpal | M1 audio tasks | Microphone and playback through Core Audio |
| rubato | M1 audio tasks | Resampling with a proper filter |
| ort | M1 engine tasks | ONNX Runtime in process (D-14 rule: final at the M0-19 memo) |
| tokio | M1 coordinator tasks | Async tasks outside the audio callback |
| tauri-cli | M0b-01 | `cargo tauri dev` and the signed local bundle for permission spikes; a build tool, not shipped. Not installed by default; install with the owner's approval |

## UI

| Item | Value | Status |
|---|---|---|
| Framework | React 19, TypeScript, Vite | Rule set: React unless Svelte is at least 30% lighter in idle memory (M0b) |
| IPC | `@tauri-apps/api` package (`invoke`, `listen`); the `window.__TAURI__` global is off (withGlobalTauri false) | Changed from the global bridge after the skeleton review |

## UI packages (apps/ui/package.json, versions installed 2026-11-17)

Ranges are in `package.json`; the lock file pins the exact version.

| Package | Installed | Why |
|---|---|---|
| react, react-dom | 19.3.1 | UI framework |
| @tauri-apps/api | 2.12.5 | `invoke` and `listen` for the Tauri shell (Apache-2.0 or MIT, no dependencies); keep its minor version equal to the `tauri` crate (2.12) |
| typescript | 5.9.3 | Strict typing; `npm run typecheck` |
| vite | 6.4.6 | Dev server and build (vite 6.4) |
| @vitejs/plugin-react | 4.7.0 | React transform for vite |
| vitest | 5.1.0 | Unit and interaction tests (`npm test`); vitest 5 |
| jsdom | 31.0.1 | DOM for vitest |
| @testing-library/react | 16.3.3 | Render and query components by role and text |
| @testing-library/user-event | 14.6.12 | Realistic user input in tests |
| @playwright/test | 1.65.1 | End-to-end journeys against the mock transport, using the installed system browser (`channel: "chrome"`); no browser download. Out of the gate for now (`scripts/check.sh e2e`) |
| eslint, @eslint/js | 9.39.5 | Lint (flat config) |
| typescript-eslint | 8.73.0 | TypeScript lint rules |
| eslint-plugin-react-hooks | 7.1.1 | Hook rules. One file-level exception is recorded in `docs/coding-style.md` |
| prettier | 3.10.1 | Format |
| @types/node | 22.20.5 | Node types for the config files and tests |
| @types/react, @types/react-dom | 19.3.0 | React types |

## Rust tooling

| Item | Value |
|---|---|
| Toolchain | pinned in `rust-toolchain.toml` (1.99.1, with clippy and rustfmt) |
| Lints | `[workspace.lints]` in the root `Cargo.toml`: no unsafe code, no `unwrap`, `todo!` or `dbg!` outside tests |
| Gate | `scripts/check.sh` (see `docs/definition-of-done.md`), also run by `.github/workflows/ci.yml` |

## Models (not in the repo)

This is the one place in the docs that names models. Licence attribution is in
`NOTICE`; the licence reads are in `docs/m0/candidates.md`.

| Stage | Model | Licence | Status |
|---|---|---|---|
| Speech gate | Silero VAD (ONNX, about 2 MB) | MIT | Rule set |
| Vietnamese recognition | PhoWhisper small, base as fallback | BSD-3-Clause | Rule set |
| Translation | opus-mt vi-en and en-vi (about 289 MB each) | Apache-2.0 tag; training data CC BY-NC-SA 4.0 | Rule set |
| Voices | ZeroTTS (Vietnamese and English, ONNX) | MIT | Rule set |
| Speaker model | chosen at M0-15 | by licence | Later |
| Summary model | chosen at M3-03 | by licence | Later |

## Adding a dependency

Add a row to the right table with the task ID and one line on why. A crate
or package with no row here is not allowed in the build.
