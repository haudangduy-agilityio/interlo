## Task

Task ID: <!-- for example M1-08, E1-03, X-10, BUG-012 -->

One sentence on what changes and why.

## Acceptance criteria

| AC | Behaviour | Test (name or path) | Result |
|---|---|---|---|
| AC-1 | | | |

## Evidence

Commands you ran on the final commit, with real output. For a change that a
user sees, add a screenshot of each state in the AC table.

Verified commit: <!-- short hash -->

## `scripts/check.sh` output

```text
paste the summary table of `scripts/check.sh full` here
```

## What the checks miss

For each check that ran green: one property it proves and one it does not.

## Decisions and assumptions

Defaults you picked, what you left out of scope, and any place you read the
requirement a different way. Write `N/A` if there are none.

## Checklist

- [ ] A test for each rule the change adds or changes
- [ ] `docs/guardrails.md` is not broken
- [ ] A change to a command or an event updates `docs/ipc-contract.md`, the Rust DTOs and `apps/ui/src/ipc/types.ts` together
- [ ] A fact in `docs/` that the change touched is updated
- [ ] No model, audio file or database is committed
- [ ] Every commit is signed off (DCO)
