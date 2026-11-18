# 0012. Model manager: pinned manifest, own public host, no Interlo server

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-60

## Context

Models are not bundled (ADR 0003) and the owner downloads each one (ADR 0006).
The app needs one design for where a model comes from, how the file is checked,
and what the UI may ask for. Interlo runs no server in v1 (ADR 0002), and the
webview must never choose a URL.

## Decision

- Each model is downloaded from the model's own public host. Interlo runs no
  server.
- The URL and the SHA-256 of each model are pinned in a manifest in the
  repository. The manifest is a compiled-in table in `crates/model-store`.
- The manager resumes an interrupted download and checks free disk space before
  it starts.
- Files are stored in `~/Library/Application Support/Interlo/models`.
- Progress is an IPC event, `models://progress`, with `{ id, receivedBytes,
  totalBytes, state }`. The state is `downloading`, `verifying`, `ok` or
  `error`.
- `ModelInfo` carries `licence` (text) and `nonCommercial` (boolean) next to
  `id`, `name`, `sizeBytes` and `state` (D-47).
- `download_model` takes `{ id, licenceAccepted }`. With `licenceAccepted`
  false it returns `INVALID_INPUT` ("Accept the licence first."). An unknown id
  returns `INVALID_INPUT`.
- A new command `cancel_model_download` takes `{ id }`.
- A file that fails its checksum is removed and never runs (guardrail 7).

## Consequences

- No Interlo server is needed to run, host or pay for. The owner downloads
  straight from the model's host.
- A model host change (a moved URL or a new file) needs a manifest change and a
  release.
- The licence screen text comes from the manifest, so each model needs a
  licence text and a non-commercial flag there (ADR 0011).
- The contract change (ModelInfo, `download_model`, `cancel_model_download`,
  `models://progress`) is recorded in `docs/ipc-contract.md`. The commands also
  go into the build command list, the capabilities file and the security
  config test.
- ADR 0012 is written before M1-04, which builds on it.
