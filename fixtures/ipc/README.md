These JSON files are sample payloads for every command result, error and event in `docs/ipc-contract.md`.
`args/` holds the arguments of the commands that take any: the keys are the wire (camelCase) names.
The Rust contract test (`apps/desktop/src-tauri/tests/ipc_contract.rs`) and the UI contract test both read them.
`ipc_error_start_check_failed.json` is the only error with `details`; every other error omits the field.
`setup_status.json` must satisfy the readiness rule in `docs/ipc-contract.md`: both tests recompute `canStart` and `missingGroups` from `items`.
`events/models_progress.json` is the `models://progress` payload. `ipc_error_licence_not_accepted.json` is the error `download_model` returns when `licenceAccepted` is false. `ipc_error_already_downloading.json` is the error `download_model` returns while a download of the same pack runs.
