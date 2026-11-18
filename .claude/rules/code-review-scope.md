# Code Review Scope

Matches the "Review" section of `docs/workflow.md`. If the two disagree, `docs/workflow.md`
wins; fix this file.

Reviews start only after the gate passes. The quality review runs on every diff. The
other reviewers run only when the diff triggers them. A reviewer that the analysis does
not call for does not run: do not launch it to be safe.

## Step 1: analyze the MR (always first)

Before any reviewer runs, the main session:

1. Lists the changed paths: `git diff --name-status <target>...HEAD`.
2. Counts the changed lines: `git diff --shortstat <target>...HEAD`.
3. Classifies the change with the table below.
4. Writes the decision, with a reason for every reviewer it adds or leaves out, to
   `docs/agent-tasks/<TASK-ID>/REVIEW-SCOPE.md` (the folder is excluded from git).

Only then does the quality reviewer run, and then the others that the file names. If the
task has no ID yet, ask the owner for one; do not invent it.

### `REVIEW-SCOPE.md` format

```markdown
# Review scope: <TASK-ID>

Head: <short hash>   Target: <branch>   Files: <n>   Lines: +<added> -<removed>
Class: <one row from the classification table>

| Reviewer | Run | Reason (trigger path or rule, or why it is not triggered) |
|---|---|---|
| Quality | yes | always |
| Security | yes / no | ... |
| Code (correctness) | yes / no | ... |
| Performance | yes / no | ... |
| Independent (second model) | yes / no | ... |

Changed paths:
- <path>
```

The file is evidence that the scope was decided before the review. If the diff changes
after the file is written, redo step 1 and update the head.

## Which reviewer runs when

| Reviewer | Agent | Run when the diff |
|---|---|---|
| Quality | `quality-reviewer` | Always |
| Security | `security-reviewer` | Touches the policy gate (`crates/meeting-core/src/policy.rs`), `crates/model-store`, `crates/quick-actions`, `crates/settings` secrets, IPC commands or events, `capabilities/`, `tauri.conf.json`, CI workflows, hooks, or anything that reads input from the webview, a file or the network |
| Code (correctness) | `code-reviewer` | Changes a decision rule or a crate "must never" rule in `docs/architecture.md`: segmenter, playback queue, capture plan, store schema, journal, retry policy, citation check |
| Performance | `performance-reviewer` | Touches `crates/audio-io`, `crates/playback`, `crates/engine-host`, store queries or indexes, IPC payload size or event rate, or UI render paths for captions |
| Independent (second model) | none; the main session runs it | Changes the policy gate or the IPC contract, prepares a release, repeats a finding that a reviewer raised before, or has a disputed verdict |

A diff that touches only docs, tests or formatting gets the quality review alone.

## Classification

Use the first row that matches. When a diff matches several, take the union of their
reviewers.

| Class | Reviewers |
|---|---|
| Docs, tests or formatting only | Quality |
| React screen or UI code, no render path for captions, no IPC change | Quality |
| UI render path for captions, or a state or list that updates per caption | Quality, performance |
| Rust code inside one crate that changes no decision rule and does not touch a trigger path | Quality |
| Decision rule or crate "must never" rule (segmenter, playback queue, capture plan, store schema, journal, retry policy, citation check) | Quality, code |
| Policy gate, model download or checksum, secrets, quick actions | Quality, security, independent when it changes the policy gate |
| IPC command or event, `capabilities/`, `tauri.conf.json` | Quality, security, independent when it changes the IPC contract. Add performance if the payload size or event rate changes |
| Audio, playback, engine host, store query or index | Quality, performance. Add code if a decision rule changes. Add security if it opens a file, a device or a process |
| CI workflow, hook or guard in `.claude/` | Quality, security |
| Release preparation | Quality, independent |

## Rules

- The reasons in `REVIEW-SCOPE.md` cite a path or a rule from the table. "Might matter"
  is not a reason to run a reviewer, and it is not a reason to skip one either: when the
  paths do not decide it, ask the owner.
- Run the reviewers that are called for in parallel, read-only, on the same diff. Every
  finding carries `file:line` evidence and a severity.
- Cross-check every finding against the code and give it a verdict, confirmed or rejected,
  with the line that proves it. A finding that two reviewers report is confirmed. Record
  rejected findings with their reason.
- After fixes, run again only the reviewers that raised findings.
- Reviewers use the `sonnet` model; the explorer uses `haiku`. Do not use `opus` for a
  review agent.
- The independent review uses a different model from the one that wrote the code. Never
  name the tool or the model behind it in an MR, a commit or a repo file.
- Give each reviewer the diff command, the docs that apply and the question to answer. Do
  not paste the whole diff into the brief.
- A reviewer that finds nothing says what it checked and what it could not check.
