# Coding style

## Rust

- Format with `cargo fmt`. Lint with `cargo clippy --workspace --all-targets -- -D warnings`.
- Each crate's `lib.rs` starts with a doc comment: what the crate owns and its
  "must never" rule from `docs/architecture.md`.
- Errors: one `thiserror` enum per crate or area. Return `Result`.
- Code not built yet returns `Err(NotImplemented)`. No `todo!()`, no
  `unimplemented!()`.
- No `unwrap()` or `expect()` outside tests, except for an invariant that a
  comment states, with a narrow `#[allow(clippy::expect_used)]` on that function.
  Files in `tests/` that have helper functions start with
  `#![allow(clippy::unwrap_used, clippy::expect_used)]` and a one-line reason,
  because the test exemption covers only `#[test]` functions.
- Pass time, randomness, hashers, recognizers and network clients in as
  parameters or traits. A rule is a pure function where possible.
- Constants carry their unit in the name and say "starting value" in a comment
  when a measurement will set them.
- No `unsafe` without a comment that states why and a test that covers it.
- Real-time callbacks: no allocation, no lock, no log, no blocking call.

## TypeScript and React

- Strict TypeScript. No `any`; use `unknown` and narrow it.
- Function components only. Never define a component inside another component.
- Do not copy props into state with `useEffect`.
- Clickable things are `<button>` or `<a>`, never a `div` with `onClick`.
  Icon-only buttons have an `aria-label`.
- The UI calls the core only through `src/ipc/client.ts`.
- Every screen that loads data shows loading, empty, error and done states.
  Status messages use `role="status"`; failures stay on the page, not only in a toast.
- No new npm package without a row in `docs/tech-stack.md`.
- One lint exception: `react-hooks/set-state-in-effect` and
  `react-hooks/exhaustive-deps` are off for `apps/ui/src/shared/useAsync.ts`
  only (set in `apps/ui/eslint.config.js`). `useAsync` is the one data-loading
  hook: callers pass `deps` on purpose, and it resets to loading when they
  change. Every other file keeps both rules on.

## Comments

- A comment states a constraint the code cannot show (a rule, a unit, a reason).
- No comments that repeat the code, narrate a change, or hold old code.
