# Tech stack

Status values: **Decided** (do not change without a new decision), **Rule set**
(the choice follows a measured rule), **Later** (added in the task that needs it).

## Platform

| Item             | Value                                                                                                                                             | Status                   |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| Operating system | macOS 14.4 or later (Core Audio process tap needs 14.4)                                                                                           | Decided (D-41)           |
| Hardware         | Apple silicon only in v1                                                                                                                          | Decided (D-41)           |
| Windows, Linux   | After v1 (phase P6)                                                                                                                               | Decided (D-41)           |
| Distribution     | A DMG signed with a self-made certificate, not notarized, no store; an install guide in the README; signed automatic updates from GitHub Releases | Decided (ADR 0022, D-61) |
| Licence          | MIT for the code; models are not bundled                                                                                                          | Decided (D-39)           |

## Languages and tools

| Item   | Version on the owner's Mac (2026-11-16) | Notes                                   |
| ------ | --------------------------------------- | --------------------------------------- |
| Rust   | 1.99.1 (edition 2021)                   | Workspace in the repo root              |
| Node   | 26.5.0, npm 11.17.0                     | UI build only                           |
| Python | 3.14.7                                  | Measurement scripts only, never shipped |

## Rust crates in use

| Crate                  | Used by                                                                                    | Why                                                     |
| ---------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------- |
| serde, serde_json      | the crates that need them (not engine-host, model-store, playback, quick-actions or store) | Event and IPC serialization; audio-io parses the device list tool output |
| thiserror              | all                                                                                        | Error enums                                             |
| rusqlite (bundled)     | meeting-core, store                                                                        | SQLite with WAL, journal, projections, full-text search |
| tauri 2, tauri-build 2 | apps/desktop                                                                               | App shell, commands, event channels                     |
| tauri-plugin-dialog 2  | apps/desktop                                                                               | The native confirmation dialog before a model download (ADR 0006). Called from Rust only, in `native_confirm.rs`; the main window is granted no `dialog:` permission. Added in M1-04.2 with the owner's approval (2026-11-25); `cargo deny check` passes. Brings in `rfd`, `tauri-plugin` and `tauri-plugin-fs` (MIT OR Apache-2.0), and moves `tauri-utils` from 2.10.3 to 2.10.4 in the lock file |
| cpal `=0.18.2`         | audio-io                                                                                   | Microphone capture through Core Audio, with the default input configuration of the chosen device; exact pin (Apache-2.0) |
| rustix 1.1             | apps/desktop                                                                               | The current user id for the owner check of model files, without `unsafe`; feature `process` only. Added in SL-01 with the owner's approval (2026-11-26). Version 1.1.6 was already in the lock file (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT) |
| rubato `=5.1.0`        | audio-io                                                                                   | Conversion of the microphone input to 16 kHz in `RateConverter`, without allocation in the audio callback; exact pin (MIT OR Apache-2.0) |
| sha2 0.11              | model-store                                                                                | SHA-256 of the model files in `check_pack`. Added in SL-01 with the owner's approval (2026-11-26) as 0.10; moved to 0.11 the same day with approval, with a dev profile override in the root manifest, because 0.10 hashed far more slowly. Version 0.11.1 was already in the lock file, so no new crate enters it (MIT OR Apache-2.0) |

## Rust crates added later (Later)

| Crate                        | Task                 | Why                                                                                                                                                                                                                                                        |
| ---------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ort | M1 engine tasks (first product use: M1-36) | ONNX Runtime in process (D-14 rule: final at the M0-19 memo). Version `2.0.0-rc.15`, a release candidate, the only line on crates.io; exact pin. First used in `bench/M1-35/` (CPU only). Used by `engine-host` since M1-36 (voice activity adapter): one process-wide environment, one session per adapter, one intra-op thread for that small model. The ONNX runtime objects need `___isPlatformVersionAtLeast`, which the Rust runtime drops under thin LTO, so `crates/engine-host/build.rs` links `libclang_rt.osx.a` of the C toolchain (found with `xcrun clang`, macOS only; the Xcode command line tools are needed to build). The recognition adapter (M1-37) shares the environment and runs its two sessions with four intra-op threads. The build script of its `ort-sys` crate fetches the runtime library once at build time from host `cdn.pyke.io` over https only, and checks the archive against a hash pinned in `ort-sys` (`dist.tsv`); the macOS system framework `CoreML.framework` is linked, and no CoreML provider is registered. Licence MIT OR Apache-2.0 |
| tokio                        | M1 coordinator tasks | Async tasks outside the audio callback                                                                                                                                                                                                                     |
| tauri-cli                    | M0b-01               | `cargo tauri dev` and the signed local bundle for permission spikes; a build tool, not shipped. Installed on 2026-11-20 with the owner's approval, pinned to `^2.12` with `--locked`. Any other install or version change needs the owner's approval again |
| objc2 0.6.5                  | M0b-05, M0b-06       | Objective-C runtime types for the Core Audio process tap spike; exact pin, used only in `bench/M0b-05/` and `bench/M0b-06/` (MIT)                                                                                                                                              |
| objc2-foundation 0.3.2       | M0b-05, M0b-06       | Foundation arrays, numbers, strings and dictionaries for the tap description and the aggregate device; exact pin, `bench/M0b-05/` and `bench/M0b-06/` only (MIT)                                                                                                               |
| objc2-core-audio 0.3.2       | M0b-05               | Core Audio bindings, including the process tap (`CATapDescription`, `AudioHardwareCreateProcessTap`); exact pin, `bench/M0b-05/` only (Zlib OR Apache-2.0 OR MIT)                                                                                          |
| objc2-core-audio-types 0.3.2 | M0b-05               | Core Audio buffer and format types used by the capture callback; exact pin, `bench/M0b-05/` only (Zlib OR Apache-2.0 OR MIT)                                                                                                                               |
| tauri =2.12.6, tauri-build =2.7.5 | M0b-06 | Window flags of the panel spike (always on top, click-through, visible on all workspaces); exact pins, the versions of the root lock file, `bench/M0b-06/` only. Default features off; features `wry`, `compression` and `custom-protocol` (the panel page loads from embedded assets, no dev server). No `macos-private-api`. (Apache-2.0 OR MIT) |
| objc2-app-kit 0.3.2 | M0b-06 | The frontmost application, the display frames and scale, the native non-activating panel of the spike; exact pin, default features off, only the feature flags of the types used; `bench/M0b-06/` only (Zlib OR Apache-2.0 OR MIT) |
| objc2-core-graphics 0.3.2 | M0b-06 | `CGWindowListCopyWindowInfo`: the topmost window of the frontmost application (identity and layer only, no window titles); exact pin, `bench/M0b-06/` only (Zlib OR Apache-2.0 OR MIT) |
| objc2-core-foundation 0.3.2 | M0b-06 | Typed arrays and dictionaries for the window list; exact pin, `bench/M0b-06/` only (Zlib OR Apache-2.0 OR MIT) |
| ndarray | M1-35 | Direct dependency with an exact pin `=0.17.2`, declared because SPEC M1-35 AC-02 lists it; no source file of `bench/M1-35/` calls it. The runtime crate pulls its own `ndarray` through the lock file. Used only in `bench/M1-35/` (MIT OR Apache-2.0) |
| tokenizers | M1-35, M1-37 | Text tokenizer for translation, speech recognition and voice; version `=0.23.2`, default features off so no network stack is linked, feature `onig` on (regex engine, a native C library built from source). Used in `bench/M1-35/` and, since M1-37, in `engine-host` (the recognition adapter loads `tokenizer.json`, finds the prompt ids by name, turns generated ids into text and composes it); M1-38 and M1-39 reuse the same line (Apache-2.0) |

The four `objc2` crates pull in `block2`, `dispatch2`, `libc`, `bitflags`, `objc2-core-foundation` and `objc2-encode` as transitive dependencies. They are pinned by `bench/M0b-05/Cargo.lock` and `bench/M0b-06/Cargo.lock` (built with `--locked`) and never enter the root workspace. `objc2-core-foundation` is also a direct dependency of `bench/M0b-06/`.

## UI

| Item      | Value                                                                                                        | Status                                                       |
| --------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------ |
| Framework | React 19, TypeScript, Vite                                                                                   | React stays; the Svelte comparison is dropped for v1 (D-191) |
| IPC       | `@tauri-apps/api` package (`invoke`, `listen`); the `window.__TAURI__` global is off (withGlobalTauri false) | Changed from the global bridge after the skeleton review     |

## UI packages (apps/ui/package.json, versions installed 2026-11-17)

Ranges are in `package.json`; the lock file pins the exact version.

| Package                        | Installed | Why                                                                                                                                                                             |
| ------------------------------ | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| react, react-dom               | 19.3.1    | UI framework                                                                                                                                                                    |
| @tauri-apps/api                | 2.12.5    | `invoke` and `listen` for the Tauri shell (Apache-2.0 or MIT, no dependencies); keep its minor version equal to the `tauri` crate (2.12)                                        |
| typescript                     | 5.9.3     | Strict typing; `npm run typecheck`                                                                                                                                              |
| vite                           | 6.4.6     | Dev server and build (vite 6.4)                                                                                                                                                 |
| @vitejs/plugin-react           | 4.7.0     | React transform for vite                                                                                                                                                        |
| vitest                         | 5.1.0     | Unit and interaction tests (`npm test`); vitest 5                                                                                                                               |
| jsdom                          | 31.0.1    | DOM for vitest                                                                                                                                                                  |
| @testing-library/react         | 16.3.3    | Render and query components by role and text                                                                                                                                    |
| @testing-library/user-event    | 14.6.12   | Realistic user input in tests                                                                                                                                                   |
| @playwright/test               | 1.65.1    | End-to-end journeys against the mock transport, using the installed system browser (`channel: "chrome"`); no browser download. Out of the gate for now (`scripts/check.sh e2e`) |
| eslint, @eslint/js             | 9.39.5    | Lint (flat config)                                                                                                                                                              |
| typescript-eslint              | 8.73.0    | TypeScript lint rules                                                                                                                                                           |
| eslint-plugin-react-hooks      | 7.1.1     | Hook rules. One file-level exception is recorded in `docs/coding-style.md`                                                                                                      |
| prettier                       | 3.10.1    | Format                                                                                                                                                                          |
| @types/node                    | 22.20.5   | Node types for the config files and tests                                                                                                                                       |
| @types/react, @types/react-dom | 19.3.0    | React types                                                                                                                                                                     |

`typescript` stays on 5.9.x (`~5.9.3` in `package.json`). `typescript-eslint` 8.73.0, the latest
release on 2026-11-23, declares the peer range `typescript@">=4.8.4 <6.1.0"`, so `npm ci` fails with
`ERESOLVE` on typescript 7. Dependabot ignores typescript semver-major updates for `/apps/ui`
(`.github/dependabot.yml`). The typescript 7 upgrade is deferred, not rejected: it comes back when
a `typescript-eslint` release supports typescript 7.

## UI fonts (bundled, M1-51)

Three families from the design system `tokens.json`, as local `woff2` files in
`apps/ui/src/assets/fonts/`, declared under the aliases `interlo-display`, `interlo-text` and
`interlo-mono` in `apps/ui/src/styles/fonts.css`. No font is loaded from the network. The
family names appear only here, in `NOTICE` and in the licence files beside the font files (guardrail 3).

| Role    | Family              | Licence     | Ranges bundled                    | Use                                     |
| ------- | ------------------- | ----------- | --------------------------------- | --------------------------------------- |
| display | Bricolage Grotesque | SIL OFL 1.1 | Latin, Latin extended, Vietnamese | Headings, the brand, the large subtitle |
| text    | Instrument Sans     | SIL OFL 1.1 | Latin, Latin extended             | Body text, buttons, tables              |
| mono    | JetBrains Mono      | SIL OFL 1.1 | Latin, Latin extended, Vietnamese | Timers, timestamps, tags, key caps      |

The attribution is in `NOTICE` (section Fonts: family, licence, author's copyright line, files and
checksums). The licence text of each family also sits beside the font files: `apps/ui/src/assets/fonts/display-OFL.txt`,
`text-OFL.txt` and `mono-OFL.txt`. The font files are the `woff2` files that Google Fonts serves
for the Latin, Latin extended and (where the family has it) Vietnamese ranges, downloaded on
2026-11-20 from `fonts.gstatic.com` with the owner's approval. The licence files are the
`OFL.txt` of each family's own repository, read on 2026-11-23.

Instrument Sans has no glyph for 96 letters the Vietnamese interface needs (checked on
2026-11-20 against the 149 code points of the Vietnamese alphabet with tones; the display and mono
files miss none). Those letters fall back to the system font. The owner accepted this on
2026-11-20 and keeps the design system's choice; a family that covers Vietnamese is a design
system change that the owner publishes.

## Rust tooling

| Item      | Value                                                                                                      |
| --------- | ---------------------------------------------------------------------------------------------------------- |
| Toolchain | pinned in `rust-toolchain.toml` (1.99.1, with clippy and rustfmt)                                          |
| Lints     | `[workspace.lints]` in the root `Cargo.toml`: no unsafe code, no `unwrap`, `todo!` or `dbg!` outside tests |
| Gate      | `scripts/check.sh` (see `docs/definition-of-done.md`), also run by `.github/workflows/ci.yml`              |

## Models (not in the repo)

This is the one place in the docs that names models. Licence attribution is in
`NOTICE`; the licence reads are in `docs/m0/candidates.md`.

| Stage                  | Model                                       | Licence                                       | Status   |
| ---------------------- | ------------------------------------------- | --------------------------------------------- | -------- |
| Speech gate            | Silero VAD (ONNX, about 2 MB)               | MIT                                           | Rule set |
| Vietnamese recognition | PhoWhisper small, base as fallback          | BSD-3-Clause                                  | Rule set |
| Translation            | opus-mt vi-en and en-vi (about 289 MB each) | Apache-2.0 tag; training data CC BY-NC-SA 4.0 | Rule set |
| Voices                 | ZeroTTS (Vietnamese and English, ONNX)      | MIT                                           | Rule set |
| Speaker model          | chosen at M0-15                             | by licence                                    | Later    |
| Summary model          | chosen at M3-03                             | by licence                                    | Later    |

## Adding a dependency

Add a row to the right table with the task ID and one line on why. A crate
or package with no row here is not allowed in the build.
