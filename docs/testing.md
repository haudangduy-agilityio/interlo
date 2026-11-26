# Testing

## Layers

| Layer | Where | Runs with |
|---|---|---|
| Rust unit | `#[cfg(test)]` modules inside each crate | `cargo test --workspace` |
| Rust integration (cross-crate flows) | `crates/integration-tests/tests/*.rs` | `cargo test --workspace` |
| IPC contract | `fixtures/ipc/*.json`, read by `apps/desktop/src-tauri/tests/ipc_contract.rs` and by the UI contract test | both test runners |
| UI unit and interaction | `apps/ui/src/**/*.test.ts(x)` (vitest, Testing Library, jsdom) | `npm --prefix apps/ui test` |
| End to end (out of the gate for now) | `apps/ui/e2e/*.spec.ts` (Playwright, installed system browser, mock transport) | `scripts/check.sh e2e` |
| Signed app (out of the gate: needs a display and Screen Recording) | `scripts/app-check.sh` (build flags, window, sockets, memory; typing only with `--typing`); its pure parts are tested by `scripts/app-check.test.sh` inside the gate; evidence is text only in `docs/m0/M0b-01/evidence/`, screenshots stay outside git | `scripts/check.sh app` |
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
  One exception: a test that needs a real model file and a synthetic audio file
  (for example `crates/engine-host/tests/vad_real_model.rs`) runs only when
  `INTERLO_MODELS_DIR` names the folder that holds them. Without the variable it
  prints one `SKIP` line and passes; with the variable and a missing file it
  fails. The model and audio files never enter git.
- The whole speech chain with the real models is the test
  `real_chain_captions_the_test_sentence`
  (`crates/engine-host/tests/chain_real_model.rs`): synthetic speech in, captions
  out, at least 13 of the 15 words of the sentence. Run it with the network
  denied, with `MODELS_DIR` set to the folder of the models:

  ```bash
  INTERLO_MODELS_DIR="$MODELS_DIR" scripts/network-off.sh cargo test --offline -p engine-host --test chain_real_model -- --nocapture
  ```

  Without the variable the test passes with its `SKIP` line and proves nothing.
- Journal replay: a recorded event file gives the same captions every time
  (`fixtures/protocol/`).
- Tauri commands call plain functions; test those functions, not the command
  wrapper.
- The UI runs against `src/ipc/mockTransport.ts` in unit, interaction and e2e tests.
- e2e uses roles and visible text, never sleeps or CSS selectors.
- Never weaken a test to make it pass: no `#[ignore]`, `.skip`, `.only`, or a
  removed assertion.

## Network-off check

The guard tests use a fake transport (`NetworkOff` counts attempts; feature `test-support` of `meeting-core`). To prove that they pass with the real network denied by the operating system, run them through the script:

```bash
scripts/network-off.sh cargo test -p meeting-core --offline --features test-support egress
```

The script runs a canary first: it must fail inside the sandbox and succeed outside it. A canary that reaches the network inside the sandbox, one that fails outside it too (an offline Mac), or a sandbox that cannot run a command, is NOT RUN (exit 2), never a pass. M1-08 and M4-03 reuse it. It needs a Mac with `sandbox-exec` and is not part of `scripts/check.sh`.

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
