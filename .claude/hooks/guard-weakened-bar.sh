#!/usr/bin/env bash
#
# Stop hook: refuse to end the turn while the working tree lowers the quality bar.
#
# A green test run proves the suite passes. It cannot see how the suite got green: an ignored
# test, a deleted test or a new `#[allow(...)]` all pass it. This hook runs floor-guard.mjs
# on the uncommitted changes (working tree against HEAD) and blocks when it finds one.
#
# Contract (https://code.claude.com/docs/en/hooks#stop-decision-control):
#   - `decision: "block"` and `reason` are top-level fields of the JSON on stdout.
#   - Claude Code ends the turn anyway after 8 consecutive blocks. `stop_hook_active` is
#     true while it is already continuing because of a stop hook, so this hook bows out.
#   - The hook always exits 0. Outside a git repository, or before the first commit, there is
#     nothing to gate and the hook stays silent. Inside a repository with a commit, a guard that
#     could not run (for example a corrupt index) means the changes were not checked, so the
#     hook blocks and says so rather than letting the turn end as if it were clean.
#
# Needs jq, git and node. A missing tool leaves the hook silent.

set -uo pipefail

for tool in jq git node; do
	command -v "$tool" >/dev/null 2>&1 || exit 0
done

INPUT=$(cat)

if [ "$(printf '%s' "$INPUT" | jq -r '.stop_hook_active // false')" = "true" ]; then
	exit 0
fi

GUARD="$(cd "$(dirname "$0")" && pwd)/floor-guard.mjs"

FINDINGS=$(node "$GUARD" --base HEAD 2>&1)
STATUS=$?

if [ "$STATUS" -eq 2 ] && git rev-parse --verify -q HEAD >/dev/null 2>&1; then
	jq -n --arg reason "The quality-bar guard could not run, so the changes were not checked.

${FINDINGS}

Fix the cause or tell the user. Do not treat the turn as checked." '{
  decision: "block",
  reason: $reason
}'
	exit 0
fi

[ "$STATUS" -eq 1 ] || exit 0

jq -n --arg reason "The changes lower the quality bar, so the work is not done.

${FINDINGS}

Fix the code instead: make the test pass, keep the assertion, remove the suppression, finish the stub. Do not rename or move the line to hide it." '{
  decision: "block",
  reason: $reason
}'
exit 0
