---
name: review
description: Review uncommitted changes before committing. Writes the review scope, runs the quality reviewer first, then only the reviewers the scope calls for (security, code correctness, performance, independent). Use when the user asks for a review of the working tree, asks whether changes are ready to commit, or says they are about to commit.
disable-model-invocation: true
argument-hint: "[task ID such as M1-08] [optional focus]"
allowed-tools: Agent, Bash(git diff *), Bash(git status *), Bash(git log *), Bash(git rev-parse *), Bash(git merge-base *), Read, Grep, Glob, Write(docs/agent-tasks/**)
---

# Review uncommitted changes

Task ID and focus, if the user gave them: $ARGUMENTS

Rules for this skill live in `.claude/rules/code-review-scope.md`, which matches the
"Review" section of `docs/workflow.md`. Read it first. Reviews start after the gate passes
(`scripts/check.sh full`); if the gate has not run on these changes, say so and stop.

## The changes

Staged:

!`git diff --staged --stat || true`

Unstaged:

!`git diff --stat || true`

Untracked:

!`git status --porcelain | grep '^??' || true`

## Step 1. Write the scope

Before any reviewer runs:

1. List the changed paths and count the lines (staged, unstaged and untracked files).
2. Classify the change with the table in the scope rule.
3. Write `docs/agent-tasks/<TASK-ID>/REVIEW-SCOPE.md` in the format the rule gives: the
   class, one row per reviewer with run yes or no, and a reason that cites a path or a
   trigger. The task ID comes from the argument or the branch name; if there is none, ask
   once. Do not invent one.

Do not start any reviewer before this file exists.

## Step 2. Quality first

Run the `quality-reviewer` agent on the working-tree diff. Give it the diff commands
(`git diff --staged`, `git diff`, and the untracked file list), the path of
`REVIEW-SCOPE.md`, and the focus. It always runs, whatever the class.

## Step 3. The rest, from the scope file

Read `REVIEW-SCOPE.md` again and run exactly the reviewers it marks yes, in parallel,
read-only, on the same diff:

| Row marked yes | Run |
|---|---|
| Security | `security-reviewer` |
| Code (correctness) | `code-reviewer` |
| Performance | `performance-reviewer` |
| Independent (second model) | the independent review, run by this session; never name its tool or model in the output |

A reviewer marked no does not run. If the diff changed since the file was written, redo
step 1 first.

## Step 4. Cross-check and report

Check every finding against the code and give it a verdict, confirmed or rejected, with the
line that proves it. A finding that two reviewers report is confirmed. Do not fix findings
unasked.

Output one verdict, then the merged findings, most severe first.

- **Ready to commit.** Nothing blocking. Say what ran and what the scope file skipped.
- **Minor issues.** List them. Committing is reasonable once the author has seen them.
- **Needs changes.** List what must be fixed first.

Each finding gives `file:line`, its severity (blocking, major or minor for quality; the
reviewer's own scale for the others), what is wrong, and the specific change. Open the
report with the reviewers that ran and those skipped, each with its reason from
`REVIEW-SCOPE.md`. End with what no reviewer checked.
