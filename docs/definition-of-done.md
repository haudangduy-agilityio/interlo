# Definition of done

## Gate

A change is done only when the full gate passes on the final change:

```bash
scripts/check.sh full
```

| Step | Command inside the gate |
|---|---|
| Rust format | `cargo fmt --all --check` |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust unit and integration tests | `cargo test --workspace` (includes `crates/integration-tests` and the IPC contract test) |
| UI lint | `npm --prefix apps/ui run lint` |
| UI format | `npm --prefix apps/ui run format:check` |
| UI types | `npm --prefix apps/ui run typecheck` |
| UI unit and integration tests | `npm --prefix apps/ui test` |
| UI build | `npm --prefix apps/ui run build` |
| Shell build | `cargo build -p interlo-desktop` |

`scripts/check.sh quick` runs the first seven steps; use it while you work.
End-to-end tests are out of the gate for now (owner, 2026-11-17). Run them with
`scripts/check.sh e2e` when a change touches a user journey.

## When a step fails

The change is not done. Fix the cause and run the gate again, until every step
passes. Never weaken a test, skip a test, lower a lint level or add an
`#[allow]` / `eslint-disable` to make the gate pass. A tool that is missing is
"NOT CONFIGURED", never a pass.

## After the gate passes

Run the reviews in `docs/workflow.md` (quality always; the others when
`.claude/rules/code-review-scope.md` says so). A confirmed finding sends the
change back to the gate.

## Checklist

- [ ] The change has a test for each rule it adds or changes.
- [ ] No guardrail in `docs/guardrails.md` is broken.
- [ ] A screen change covers loading, empty, error and done states, or the
      report says which state it skips and why.
- [ ] A change to a command or an event updates `docs/ipc-contract.md`, the Rust
      DTOs, `apps/ui/src/ipc/types.ts` and `fixtures/ipc/` together.
- [ ] A fact in `docs/` that the change touched is updated.
