---
name: review-handoff
description: Prepare a forge-neutral merge request description for an Interlo change after verification and review - what and why, an acceptance-criteria table, evidence tied to the head commit, the review focus for the owner - and decide BLOCKED, DRAFT or READY. Use when implementation, verify and review are done and the owner wants the MR text.
argument-hint: "[task ID such as M1-08]"
allowed-tools: Bash(git status *), Bash(git diff *), Bash(git log *), Bash(git rev-parse *), Bash(git merge-base *), Read, Grep, Glob, Write(docs/agent-tasks/**)
---

# Review handoff

Task: $ARGUMENTS

Goal: one short description that lets the owner see why, what, how it was verified and where
to look, so the owner does not repeat the review. Station 7 of `docs/workflow.md`. The
description is plain Markdown, written in English, and works on any forge. Do not use forge
syntax, a forge command, or a link that only resolves on one forge.

Preparing the text does not authorize a commit, a push, an MR or an upload. Do those only when
the owner asks, to the branch and target the owner names. Never add attribution lines to the
description. Never put a secret, a key, a transcript or any personal data in it.

## 1. Fix the head

Run `git rev-parse HEAD` and `git merge-base HEAD <target>`. Write the short head hash and
the target at the top. Every piece of evidence below carries this hash. Evidence from another
head is stale: rerun it, or mark it NOT VERIFIED. If code changes after the text is written,
redo this step and every row that the change touches.

## 2. Gates and their status

Read the task folder `docs/agent-tasks/<ID>/` (SPEC, PLAN, TDD-LOG, VERIFICATION,
REVIEW-SCOPE, REVIEWS, ACCEPTANCE when present). Each gate is PASS, FAIL, NOT RUN or N/A,
with its evidence:

| Gate | Evidence required | Where |
|---|---|---|
| TDD | RED then GREEN per slice | `TDD-LOG.md` |
| Gate | `scripts/check.sh full` exit 0 on this head | `VERIFICATION.md` |
| Behaviour | the check named in `/verify` | `VERIFICATION.md` |
| UI journeys | `scripts/check.sh e2e` or one journey, UI against the mock transport | `/verify-app` evidence. The gate does not include e2e; report it on its own line |
| Quality review | findings with verdicts. Always required | `REVIEWS.json` |
| Security, Code, Performance, Independent | only the reviewers that `REVIEW-SCOPE.md` marks yes | `REVIEWS.json` |

Rules for review gates:

- Quality runs first and always. A missing quality review is BLOCKED.
- Read `docs/agent-tasks/<ID>/REVIEW-SCOPE.md` for the rest. If it is missing, or its head
  is not this head, the scope was not decided on this change: BLOCKED until `/review` writes
  it again. Do not decide the scope again in this skill.
- A reviewer the scope file marks no is N/A, citing the file and its reason. A reviewer it
  marks yes needs a result on this head: PASS or FAIL with evidence, never N/A.
- Every finding has `file:line` evidence, a severity and a verdict (confirmed or rejected)
  from the cross-check. Record the verdict and the outcome per finding. Never record which
  side reported it, and never name the tool or model behind a review.
- A gate with one side only is not BLOCKED. Report it as PASS or FAIL on that side's result,
  say it ran with one side, and list it for the owner.
- A rejected finding stays in the record with its reason, so the next review does not raise
  it again.
- `scripts/check.sh` exit 2 (NOT CONFIGURED) is a missing check, not a pass.

## 3. Decide

- **BLOCKED.** A mandatory gate failed or is missing, a critical or high security finding is
  open, an AC is not verified, or the head moved after the evidence. Do not present it as
  ready. Fix and rerun. A collaborative Draft is allowed only when the owner asks, and it
  lists every blocker first.
- **DRAFT.** All gates pass, but a material decision belongs to the owner: product behaviour,
  an architecture trade-off, a new dependency, a file outside the PLAN, accepted risk. Name
  the decision, the options, the effect of each and who decides. Never choose silently.
- **READY.** All gates pass on this head and no material decision is open. The owner still
  inspects and approves; nothing is approved or merged automatically.

## 4. Write the description

Use these sections in this order. Drop a section that has nothing true to say.

```markdown
# <TASK-ID>: <imperative summary>

Head: <short hash>   Target: <branch>   Status: BLOCKED | DRAFT | READY

## Why
Two to four bullets. For a bug: the symptom, the root cause, before and after.
For a refactor: the behaviour that stays the same.

## What changed
Crates and screens touched, by name. Any change to a command or event lists the
contract doc, the Rust DTOs, `apps/ui/src/ipc/types.ts` and `fixtures/ipc/` together.

## Acceptance criteria
| AC | Check | Result | Evidence at <short hash> |
|---|---|---|---|
| AC-1 | test name or journey | PASS / FAIL / NOT RUN | command and its result, or a path |

## Gates
| Gate | Result | Note |
|---|---|---|

## Decision needed from the owner
Only for DRAFT. Decision, options, effect, who decides.

## Review focus
Three to five items at most: what the agents cannot settle - intent, a trade-off, a
visible change, accepted risk. Each gives a file or commit and one concrete question. Zero
items is fine. Never list a fixed finding as pending.

## Not covered
What this change does not prove: no latency measurement, e2e outside the gate, audio and
models not exercised, a state not photographed.
```

Evidence rules:

- Each row names the command and its real result. Paste short output; link or name a file for
  long output. Never paste a log wall.
- For a screen change, one screenshot per state the AC names (loading, empty, error, done,
  and dark mode or narrow width when touched), taken from the final head. A state that could
  not be photographed is written down as such. No visible change: say so.
- Do not use a screenshot of terminal output when the command and its result say the same.
- Do not add a row for pipeline status. The forge shows it. A check that did not run stays
  NOT RUN.
- Size: if the diff is far over 400 changed lines, say so and propose a split.

## 5. Report to the owner

Give the status and its reason, the path of the saved description, and the open items. When a
task ID is known, save the text as `docs/agent-tasks/<ID>/MR.md` (the folder is excluded from
git). End with one question: for READY, whether to open the MR; for DRAFT, the decision; for
BLOCKED, which station to return to.
