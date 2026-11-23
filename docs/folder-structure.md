# Folder structure

```text
Cargo.toml                 Rust workspace; every crate below is a member
crates/
  meeting-core/            protocol, session states, coordinator, journal, policy gate, start-check input rules
  audio-io/                devices, device list, setup report, capture plan, output, ring buffer, resampler trait
    src/device_list.rs       reads the device list with one fixed system tool, sorts devices into kinds
    src/setup_report.rs      the eight setup items with typed fixes and the Bluetooth warning
    examples/list_devices.rs manual run: prints the real devices of this Mac (`cargo run -p audio-io --example list_devices`)
    tests/slow_consumer.rs   a stalled consumer never blocks capture and drops are counted
  engine-host/             engine traits, registry, segmenter, language gate
  playback/                playback queue, speed-up, mixer
  store/                   SQLite projections, schema, full-text index
  summary-jobs/            job queue, citation check, question answerer
  model-store/             model specs, size and checksum checks, download gate
  settings/                settings values, apply timing, hotkeys
  diagnostics/             log events, redaction
  workspace-notes/         notes, tasks, export, auto-delete (E1)
  quick-actions/           translate, fix grammar, save note in any app (E2)
  integration-tests/       cross-crate tests only; ships nothing
    tests/capture.rs         capture plan and audio-io flows
    tests/journal_replay.rs  recorded event files give the same captions
    tests/pipeline.rs        engine chain through the coordinator
    tests/records.rs         meeting records and summaries
    tests/settings.rs        settings values and hotkeys
    tests/delete_meeting.rs  deleting a meeting scrubs the store and the journal files
apps/
  desktop/src-tauri/       Tauri shell
    src/commands/          one file per command group
    src/clock.rs           time source for the shell
    src/first_paint.rs     shows the hidden main window after a delay if the page did not (safety net)
    src/native_confirm.rs  native dialog that proves an owner click (stub, ADR 0006)
    src/navigation.rs      webview navigation rules (only the app itself)
    tests/                 IPC contract, security config, demo-data gate, native confirm gate,
                           network boundary
    build.rs               registers the command list for the capability check
    tauri.conf.json        window, CSP and bundle settings
    src/ipc.rs             command DTOs and IpcError
    src/events.rs          event names and payloads
    capabilities/          window permissions
    icons/                 bundle icons (copied from assets/icon)
  ui/                      React UI (Vite)
    src/main.tsx           entry point
    src/App.tsx            root component
    src/protocol.ts        caption and session protocol types used by the UI
    src/app/               shell (sidebar, top bar), navigation, app state, setup status, error boundary, language sync
    src/styles/            tokens.css (the only file with literal colours, radii, shadows, sizes), fonts.css
    src/ipc/               contract types, transports (Tauri, mock), typed client, runtime guards
    src/features/<name>/   one folder per screen or panel
      live/                  also holds CaptionList and the session notices (SessionBanner, StartingNotice) that the subtitle bar and compact panel reuse
      models/                also holds DownloadProgress and ModelDownloadControl, the Download or progress control that the Models tab and the setup list share
    src/shared/ui/         shared components (Banner, Button, Chip, EmptyState, Meter, ...)
    src/shared/useAsync.ts the one data-loading hook (loading, error, done)
    src/shared/clock.tsx   clock context for components that show time
    src/shared/useActionFailure.ts  turns a failed action into a banner
    src/shared/format.ts   date and number formatting
    src/shared/i18n.ts     t(key), language provider; messages.en.ts and messages.vi.ts hold the text
    src/shared/useFocusTrap.ts  Tab wrap for dialogs and the settings sheet
    src/shared/testing/    test helpers (renderApp.tsx); never imported by app code
    e2e/                   Playwright journeys (one file per journey)
bench/<task-id>/           throwaway spike or measurement crate (M0b-05, M1-35); its own [workspace], not a member of the root workspace; merged with its note docs/m0/<task-id>.md
assets/icon/               icon sources
docs/                      reference files (routed from CLAUDE.md)
  adr/                     one file per architecture decision
  m0/                      measurement notes and candidate lists
  wireframe/               user flows (the wireframe export is still to come)
fixtures/
  protocol/                journal replay files
  ipc/                     IPC contract payloads read by Rust and UI tests
  audio/                   one text sample of the system audio tool's output (generic device names), read by audio-io tests
scripts/check.sh           the gate (quick, full), the e2e run (e2e), the signed-app check (app) and the dependency audit (supply)
scripts/app-check.sh       the steps of the signed-app check; `app-check.test.sh` tests its pure parts; `app-check/` holds the Swift helpers (window list, input source, app driver)
.claude/                   project hooks and their tests, review agents, skills, review-scope rule
.gitlab-ci.yml, .gitlab/   GitLab pipeline, merge request and issue templates, code owners
.github/                   GitHub CI workflow, templates and Dependabot; used after the move to GitHub
root files                 LICENSE, NOTICE, SECURITY, CONTRIBUTING, CHANGELOG,
                           clippy.toml, rustfmt.toml, rust-toolchain.toml, .editorconfig, .gitattributes
```

## Where new code goes

- **Logic** goes into the crate that owns it in `docs/architecture.md`. A new crate
  only when no crate owns the topic; add it to the root `Cargo.toml` members and to
  this file and `docs/architecture.md`.
- **Tauri commands** go into `apps/desktop/src-tauri/src/commands/<group>.rs`. The
  command calls a plain function, so tests need no Tauri runtime.
- **A new screen** gets `apps/ui/src/features/<name>/` with its screen component,
  its `index.ts`, and its tests.
- **A shared component** goes into `apps/ui/src/shared/ui/` only when two features
  use it.

## Never in the repo

Models, audio files, databases, `target/`, `node_modules/`, `apps/ui/dist/`,
`apps/desktop/src-tauri/gen/`. The `.gitignore` covers them.
