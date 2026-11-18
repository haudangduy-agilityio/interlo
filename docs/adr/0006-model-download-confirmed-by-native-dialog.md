# 0006. Model download is confirmed by a native dialog in Rust, not by the webview

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-52

## Context

A model download is the only network use allowed in v1 and it needs the owner's
click (guardrail 7). The download check in `crates/model-store` takes a boolean,
`owner_clicked`. If the webview supplies it, the flag proves nothing: any script
that runs in the webview can send it as true. A URL that came from the webview would also let that
script pick the host.

## Decision

The Rust shell shows a native confirmation dialog before it starts a download.
The shell resolves the model id against a model table compiled into the app and
never takes a URL from the webview. The shell refuses the download unless the
owner accepted the model's licence. The policy gate then checks the host of the
compiled-in URL against the model-host allow list.

## Consequences

- A compromised or buggy webview cannot start a download or choose a host.
- The click is proven by code the webview cannot call, so `owner_clicked` is
  set from the dialog result and never from a webview value.
- Adding a model needs a new app build, because the table is compiled in.
- A test needs a fake confirmation and a fake table; the dialog itself is
  checked by hand once per release.
- The download command and its arguments are in `docs/ipc-contract.md`. The
  model manager design is in ADR 0012.
- Work item: M1-04.

- Implementation status: the `OwnerConfirmation` token (model-store) can only be built
  by `from_native_dialog`, and a test fails if any other source file calls it. The
  native dialog in `apps/desktop/src-tauri/src/native_confirm.rs` is a stub that
  returns no confirmation, so no download can start yet. The `licenceAccepted`
  argument from the webview proves nothing about a click.
