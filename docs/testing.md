# Testing

## Layers

| Layer | Where | Runs with |
|---|---|---|
| Rust unit | `#[cfg(test)]` modules inside each crate | `cargo test --workspace` |
| Rust integration (cross-crate flows) | `crates/integration-tests/tests/*.rs` | `cargo test --workspace` |
| IPC contract | `fixtures/ipc/*.json`, read by `apps/desktop/src-tauri/tests/ipc_contract.rs` and by the UI contract test | both test runners |
| UI unit and interaction | `apps/ui/src/**/*.test.ts(x)` (vitest, Testing Library, jsdom) | `npm --prefix apps/ui test` |
| End to end (out of the gate for now) | `apps/ui/e2e/*.spec.ts` (Playwright, installed system browser, mock transport) | `scripts/check.sh e2e` |
| Latency and audio | measurement harness (M1-26) | not in the gate yet |

## What to test

- Every rule gets a test: a decision rule (capture order, start gate), a limit
  (speed-up cap 1.15, 30 s hold, 1000 words), a refusal (system hotkey, checksum
  mismatch, password field), or a format (IPC JSON names, export text).
- Test the boundary values of a limit, not only one value inside it.
- A bug fix starts with a test that fails on the old code.
- An e2e test covers one user journey from the wireframe (`docs/wireframe/user-flow.md`).

## How

- No sound card, no model and no network in tests. Clocks, hashers, engines and
  network clients are passed in as parameters or traits.
- Journal replay: a recorded event file gives the same captions every time
  (`fixtures/protocol/`).
- Tauri commands call plain functions; test those functions, not the command
  wrapper.
- The UI runs against `src/ipc/mockTransport.ts` in unit, interaction and e2e tests.
- e2e uses roles and visible text, never sleeps or CSS selectors.
- Never weaken a test to make it pass: no `#[ignore]`, `.skip`, `.only`, or a
  removed assertion.

## Updater and driver installer tests

These tests are expected when the code lands (M4-15 for the updater, M1 for the
installer). They use a fake clock, a fake manifest, a fake download and a fake
installer; none needs the network.

- An update with a wrong signature is refused.
- No update download starts while a meeting runs; the check waits until the
  meeting ends. The "Check for updates" button follows the same rule.
- With "Update automatically" off, a new version is reported and nothing is
  downloaded.
- The app never restarts to update while a meeting runs.
- The update check sends only the request for the manifest file: the fake
  client sees no machine id, user id or usage data.
- A driver installer whose SHA-256 does not match the manifest is refused and
  never runs.
- The installer URL comes from the compiled-in manifest, never from the webview.
- The loopback test and the setup check run again after the device appears;
  a device that does not appear within 15 s gives a plain failure message.

## Names

- Rust: `fn <rule>_<expected_result>()`, for example
  `merge_short_pause_without_final_mark`.
- UI: `it("<what the user sees> when <condition>")`.
- e2e: one file per journey, `<journey>.spec.ts`.

## Fixtures

Text only, or synthetic audio. Synthetic audio is made from the reading list
(`docs/m0/reading-list.md`) or from text only. No recording of a human voice
goes into the repository. A recording used for scoring is deleted after
scoring (D-51). Tests that need speech use synthetic audio or a live run, not
stored recordings.
