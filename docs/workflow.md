# Workflow (SDLC)

How a task moves from an idea to a release. Each station has an owner, an
output, and a check. A station starts only when the station before it has its
output on record. Human gates stay closed until the owner answers.

## Pick the route

| Situation | Route |
|---|---|
| Rough idea, options unclear | brainstorm > epic > ticket route |
| Code area unknown | scout > epic |
| Large feature, several tickets | epic (breakdown into tickets and waves) > ticket route per ticket |
| One ticket, clear behaviour | ticket route |
| Small bug with a repro | diagnose > TDD > verify > review |
| Speed or size target with a number | perf loop > verify > review > MR |
| Merged MRs to ship | release |
| After merges | post-merge sweep > bug tickets |

Ticket route: grill SPEC > SPEC gate > PLAN > TDD > verify > review > MR > acceptance.

## Stations

| # | Station | Output | Check before the next station |
|---|---|---|---|
| 1 | Grill the SPEC | `SPEC.md` with numbered acceptance criteria (AC-1, AC-2, ...) | Every AC is testable and names its UI state or rule |
| 2 | SPEC gate | Quality-gate verdict | **Owner approves the SPEC** |
| 3 | PLAN | `PLAN.md`: files in scope, tests per AC, risks | **Owner approves the PLAN** |
| 4 | TDD | Code and tests, RED then GREEN then REFACTOR per slice | `TDD-LOG.md` shows each RED and GREEN run |
| 5 | Verify | `VERIFICATION.md`: `scripts/check.sh full` output (format, lint, unit, integration, build), plus the app run for any UI or IPC change | Every step passes on the current head; a failure goes back to station 4 until it passes |
| 6 | Review | `REVIEWS.json`: findings with evidence and verdicts | No blocking finding open |
| 7 | MR | MR description with the AC table and evidence | **Owner opens or approves the MR** |
| 8 | Acceptance | `ACCEPTANCE.md`: each AC run on the MR head, without the developer's evidence | Every AC is ACCEPTED |
| 9 | Merge | Merge commit | **Owner merges** |
| 10 | Release | Tag, notarized build, release notes | **Owner names the tag** |

## Rules

- A failed check goes back to the station that owns the fix. Verify, review and
  acceptance failures go back to TDD. Never skip forward.
- The same failure three times at one station stops the ticket. The owner decides
  whether the approach is wrong.
- A SPEC or PLAN change after approval needs a new approval.
- A new dependency, a scope change or a file outside the PLAN pauses the ticket
  for the owner.
- One implementer per ticket. Reviewers are read-only and independent of the
  implementer.
- Two tickets that touch the same files run one after the other.
- Missing environment, missing tool or an unverified AC is BLOCKED, never PASS.

## Ticket tracking (GitHub)

The tracker is GitHub issues in `haudangduy-agilityio/interlo`; the review unit is a
GitHub pull request (called MR in this doc). The owner's login is
`haudangduy-agilityio`. Status is a label, so one filter shows what is being worked on.

| Status label | Set when | Set by |
|---|---|---|
| `in-progress` | The implementer picks the ticket | At pick, together with the assignee |
| `in-review` | The PR is open | At PR open; removes `in-progress` |
| `done` | The PR is merged | After merge; removes `in-review` and closes the issue |

Steps for every ticket:

1. **Pick.** Assign the issue to `haudangduy-agilityio` and add `in-progress`. One
   ticket at a time per implementer; a ticket with `in-progress` is taken.
2. **Open the PR** after verify and review pass. Assign the PR to
   `haudangduy-agilityio`, request a review from them, link the issue with
   `Closes #N`, and copy the issue labels. If GitHub refuses the review request
   (the author cannot review their own PR), say so in the PR and go on.
3. **Swap the label** on the issue: remove `in-progress`, add `in-review`.
4. **After merge** (the owner merges, station 9): remove `in-review`, add `done`,
   and close the issue if it is still open.

Every ticket is assigned to the owner at creation. A bug ticket carries the `bug`
label next to its `ft::`, `sprint::` and `priority::` labels. Never leave two
status labels on one issue.

## Review

Reviews start only after the gate passes. Reviewer agents are in `.claude/agents/`.

**Always: quality review.** Code quality of the diff: readability, naming
(`docs/naming-conventions.md`), style (`docs/coding-style.md`), duplication, dead
code, comment hygiene, tests that cannot fail, and drift from `docs/`.

**Only when the diff needs it: security, code, performance, independent.**
Before any of these, analyze the merge request (step 1 below) and write the
decision with its reason in `docs/agent-tasks/<TASK-ID>/REVIEW-SCOPE.md`.
A reviewer that the analysis does not call for does not run.

1. **Analyze the MR.** List the changed paths, count lines, and classify the
   change with the table in `.claude/rules/code-review-scope.md`.
2. **Pick reviewers.** Quality always. Add the others only for the triggers below.
3. **Run them read-only**, each on the same diff. Every finding carries file:line
   evidence and a severity.
4. **Cross-check.** Confirm or reject each finding against the code, with the line
   that proves it. A finding two reviewers report is confirmed.
5. **Fix and re-run.** A confirmed finding goes back to station 4 and then the gate;
   only the reviewers that raised findings run again.

| Reviewer | Run when the diff |
|---|---|
| Quality | Always |
| Security | Touches the policy gate (`crates/meeting-core/src/policy.rs`), `crates/model-store`, `crates/quick-actions`, `crates/settings` secrets, IPC commands or events, `capabilities/`, `tauri.conf.json`, CI workflows, hooks, or anything that reads input from the webview, a file or the network |
| Code (correctness) | Changes a decision rule or a crate "must never" rule in `docs/architecture.md`: segmenter, playback queue, capture plan, store schema, journal, retry policy, citation check |
| Performance | Touches `crates/audio-io`, `crates/playback`, `crates/engine-host`, store queries or indexes, IPC payload size or event rate, or UI render paths for captions |
| Independent (second model) | Changes the policy gate or the IPC contract, prepares a release, repeats a finding that a reviewer raised before, or has a disputed verdict |

A diff that touches only docs, tests or formatting gets the quality review alone.

## Where task files live

`docs/agent-tasks/<TASK-ID>/` holds SPEC, PLAN, TDD-LOG, VERIFICATION, REVIEWS,
ACCEPTANCE and DECISIONS for one task. The folder is excluded from git
(`.git/info/exclude`); the MR description carries the evidence itself.
A decision that outlives the task goes into `docs/adr/` and `docs/decisions.md`.

## Task IDs

Use the Task Breakdown IDs: `M1-08`, `E1-03`, `X-10`. Bugs found after a merge
get `BUG-NNN`.
