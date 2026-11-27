# Development

## First setup

1. Rust 1.99 or later with the clippy component, and Node 26 or later.
2. `npm --prefix apps/ui ci --ignore-scripts` (the install CI runs; use `install` only to change dependencies)
3. `cargo build --workspace` (the first build compiles Tauri; it takes several minutes).
4. Virtual audio devices and models are installed from the app's Setup screen,
   never by a build script.

## Commands

```bash
scripts/check.sh quick                      # format, lint, unit and integration tests, GitLab template check
scripts/check.sh full                       # quick + UI build + shell build + hook tests (the gate)
scripts/check.sh supply                     # dependency audit (needs: cargo install cargo-deny --locked); CI runs it in the check job and weekly (supply.yml)
scripts/check.sh e2e                        # end-to-end journeys (out of the gate for now)
scripts/check.sh app                        # the signed app: build flags, window, sockets, memory (needs an unlocked screen, Screen Recording for the app that runs it and the local signing identity; screenshots go to ../interlo-evidence/M0b-01, not into git)
scripts/check.sh app --typing               # the same, plus the Telex typing check (opt-in): it also needs Accessibility for the app that runs it and the Telex input source enabled; it sends key events to one pid and changes your input source and clipboard (text only is restored)
cargo test --workspace                      # all Rust tests
cargo test -p <crate>                       # one crate
cargo clippy --workspace --all-targets -- -D warnings   # lint
npm --prefix apps/ui test                   # UI tests
npm --prefix apps/ui run typecheck
npm --prefix apps/ui run lint
npm --prefix apps/ui run e2e                # Playwright journeys against the mock transport
npm --prefix apps/ui run dev                # UI in a browser, mock transport
npm --prefix apps/ui run build              # builds apps/ui/dist for the shell
cargo run -p interlo-desktop                # app window (needs apps/ui/dist)
```

`cargo run -p interlo-desktop` is the unbundled run. `cargo tauri dev` needs the
Tauri CLI, which is not installed by default; install it only with the owner's
approval. An unbundled run attaches microphone permission prompts to the
terminal, so use a signed local bundle for the permission spikes (M0b-05).

## Checking the microphone permission in the app

The permission belongs to the bundle only when it starts through `open`; a binary started
from a shell is judged as the terminal app (M0b-05 finding 1).

1. Build and sign the bundle: `scripts/app-check.sh bundle`.
2. Reset the permission: `tccutil reset Microphone app.interlo.desktop`.
3. Start it: `open target/release/bundle/macos/Interlo.app`. Answer the system dialog
   (Allow, or Don't Allow).
4. Leave the Setup screen and open it again: this runs a new `check_setup`. The call made
   at start, while the dialog is open, can read `error`.
5. Repeat from step 2 for the other answer. The `mic` row reads `ok`, or `missing` with a
   refused permission; the app keeps running either way.

## Developer build with local models

The feature `dev-models` makes the app read a model folder from the environment, so
that Start on the Live screen shows captions from the microphone before the model
download exists. A default build reads no folder. The folder holds `<pack>/<file>`
as the model table names them.

1. Build the bundle. `--config` turns the bundle on for this one build.

   ```bash
   cargo tauri build --bundles app --features dev-models --config '{"bundle":{"active":true}}'
   ```

2. Sign it, with `IDENTITY` set to the local signing identity.

   ```bash
   codesign --force --deep --sign "$IDENTITY" target/release/bundle/macos/Interlo.app
   ```

3. Start it through `open`, so that the microphone permission belongs to the app.
   `MODEL_DIR` is the absolute path of the model folder, `LOG_FILE` receives the
   error output. Keep `LOG_FILE` outside protected folders such as the Desktop: the
   start fails when macOS refuses to open the file.

   ```bash
   open -n target/release/bundle/macos/Interlo.app --env INTERLO_DEV_MODELS_DIR="$MODEL_DIR" --stderr "$LOG_FILE"
   ```

A model folder inside a protected folder makes macOS ask once for access to that folder.
When the `mic` row reads `missing` and no dialog appears, a refusal is already stored:
see "Checking the microphone permission in the app".

The gate builds both debug programs and checks that only the developer one holds the
variable name (`scripts/check-dev-models.sh`).

## Workflow for a task

1. Find the task in the Task Breakdown (IDs such as M1-08) and read its done check.
2. Read the owning crate or feature in `docs/architecture.md`.
3. Write the test for the rule, then the code (`docs/testing.md`).
4. Keep the change inside the owning crate or feature.
5. Finish with `docs/definition-of-done.md`.
