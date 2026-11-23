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
scripts/check.sh supply                     # dependency audit (needs: cargo install cargo-deny --locked)
scripts/check.sh e2e                        # end-to-end journeys (out of the gate for now)
scripts/check.sh app                        # the signed app: build flags, window, sockets, memory (needs a display and Screen Recording; about 6 minutes; screenshots go to ../interlo-evidence/M0b-01, not into git)
scripts/check.sh app --typing               # the same, plus the Telex typing check: it sends key events and changes your input source and clipboard (opt-in)
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

## Workflow for a task

1. Find the task in the Task Breakdown (IDs such as M1-08) and read its done check.
2. Read the owning crate or feature in `docs/architecture.md`.
3. Write the test for the rule, then the code (`docs/testing.md`).
4. Keep the change inside the owning crate or feature.
5. Finish with `docs/definition-of-done.md`.
