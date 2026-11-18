---
name: verify
description: Prove that a change works by running the Interlo gate (scripts/check.sh full), then one behaviour check, then an acceptance-criteria table. Use after implementing anything, when asked whether a change works, or before declaring a task done.
argument-hint: "[task ID such as M1-08, or what to verify]"
allowed-tools: Bash(scripts/check.sh *), Bash(cargo test *), Bash(cargo build *), Bash(cargo clippy *), Bash(npm --prefix apps/ui run test *), Bash(npm --prefix apps/ui test *), Bash(npm --prefix apps/ui run typecheck), Bash(git status *), Bash(git diff *), Bash(git log *), Bash(git stash list), Bash(git stash show *), Bash(git stash push *), Bash(git stash pop), Read, Grep, Glob, Write(docs/agent-tasks/**)
---

# Verify the change

What to verify, if the user said: $ARGUMENTS

"Looks done" is not evidence. This skill produces evidence. Station 5 of
`docs/workflow.md` owns it; `docs/definition-of-done.md` lists what must pass.

## 1. Run the gate

```bash
scripts/check.sh full
```

`full` runs format, lint, unit and integration tests, typecheck, and the UI and
desktop builds. It is the gate. End-to-end journeys are not in it for now; they run
apart with `scripts/check.sh e2e` (see `/verify-app`), and a pass of `full` says
nothing about them.

Read the exit code, not only the text:

| Exit | Meaning | Report as |
|---|---|---|
| 0 | Every step passed | the summary table, pasted |
| 1 | A step failed | FAILED, with the failing step's output. Fix it, then run the gate again from the top |
| 2 | A step is NOT CONFIGURED (a missing tool or npm script) | BLOCKED, with the step name. Never a pass |
| 64 | Bad usage | a mistake in the command; fix it |

Never suppress a lint rule, ignore a test or edit a test to get a pass. The Stop hook
blocks those moves, and so does review.

If `scripts/check.sh` is missing, stop and say so. Do not replace it by hand and call
the result the gate. You may run the six commands in `docs/definition-of-done.md` as a
partial check, and you must label the report "partial: gate script missing".

## 2. Behaviour check

A green gate proves the tests pass. It does not prove the change does what was asked.
Add one check that exercises the real behaviour, chosen by what changed:

| Change | Check |
|---|---|
| A rule or a limit in a crate | Run the named test, `cargo test -p <crate> <test_name>`, and show it. Show the boundary value as well as one inside the limit |
| A command or event | Both runners read `fixtures/ipc/*.json`. Show the contract test pass in each: `cargo test -p interlo-desktop --test ipc_contract` and `npm --prefix apps/ui test`. Confirm `docs/ipc-contract.md`, the Rust DTOs and `apps/ui/src/ipc/types.ts` changed together |
| A screen | Run `/verify-app` and read its evidence. Check the loading, empty, error and done states the change touches |
| Journal replay or captions | Run the replay test on the file in `fixtures/protocol/` and show the captions |
| Audio, latency or real-time code | The measurement harness is not in the gate yet. Say "not measured". Show the unit tests and the review of the callback rule; do not claim a latency |
| A guardrail (privacy, secrets, logs) | Show the test that fails when the rule is broken |

## 3. Regression test (bug fixes)

Run the new test before the fix and show it fail. Apply the fix and show it pass. Both
runs come from this session. To take the fix away, use `git stash push -- <file>` or a
copy of the file. Never `git checkout` a file that has uncommitted work.

## 4. Acceptance criteria

List each AC from the task's `SPEC.md` (`docs/agent-tasks/<ID>/`), or from the ticket if
there is no SPEC. One row each:

| AC | Check that proves it | Result | Evidence |
|---|---|---|---|
| AC-1 | test name or command | MET / NOT MET / NOT RUN | pasted output or a path |

"The gate passed" does not prove an AC. An AC with no check is NOT RUN.

## Report

State the outcome first: VERIFIED, NOT VERIFIED or BLOCKED. Then the gate summary, the
behaviour check, the AC table, and the commit hash the evidence belongs to
(`git rev-parse --short HEAD`). Evidence from an earlier head does not count.

- Paste what the commands returned. Never summarize a pass you did not see.
- A skipped check is reported as skipped with the reason, never as a pass.
- If a failure was there before this change, say so and name it.
- Do not claim success while any check is red.

When a task ID is known, save the report as `docs/agent-tasks/<ID>/VERIFICATION.md`.
That folder is excluded from git; the MR description carries the evidence itself.
