# 0020. IPC types stay hand-written; the fixtures keep both sides in step

- Status: accepted
- Date: 2026-11-18
- Decision ID: none (raised in a review of `main`; the owner accepted it on 2026-11-19)

## Context

The IPC contract exists three times: `docs/ipc-contract.md`, the Rust types in
`apps/desktop/src-tauri/src/ipc.rs` and `events.rs`, and the TypeScript types in
`apps/ui/src/ipc/types.ts`. A change to a command or an event edits all three in
one change. Sample payloads in `fixtures/ipc/` are read by the Rust tests and by
the UI tests, so a field that one side renames fails a test on the other side.

Generating the TypeScript types from the Rust types (for example with `ts-rs` or
`specta`) would remove the third copy. It adds a build step and a dependency to
both toolchains, and each dependency needs a row in `docs/tech-stack.md`.

## Decision

Keep the types hand-written. The fixtures are the check that the two sides agree.
Domain types stay out of the contract: a wire type such as `ipc::Direction` is a
copy with its own names and has a `From` conversion to the domain type
(`playback::Direction`), so a change to the domain type breaks the build.

## Consequences

A contract change costs three edits and one fixture. Revisit this decision when a
mismatch passes the fixture tests once, or when the contract grows to the point
that the fixtures no longer cover every field. The replacement would be
generation from the Rust types, with the generated file checked by a test.
