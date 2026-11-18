#!/usr/bin/env bash
#
# Tests for session-context.sh. Each case runs the hook inside a throwaway git repository
# (or a directory that is not one) and checks the SessionStart output it prints.
#
# Every case checks three things together: the output on stdout (empty means the hook
# stepped aside), the exit code (always 0), and that stderr is empty. Claude Code reads the
# context from stdout, so anything on stderr or a non-zero exit must fail a case.
#
# Usage: .claude/hooks/test/session-context.test.sh
# Needs jq and git.

set -uo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
HOOK="$REPO_ROOT/.claude/hooks/session-context.sh"

command -v jq >/dev/null 2>&1 || {
	echo "jq is required to read the hook's output" >&2
	exit 2
}
command -v git >/dev/null 2>&1 || {
	echo "git is required to build the test repositories" >&2
	exit 2
}
[ -x "$HOOK" ] || {
	echo "hook is missing or not executable: $HOOK" >&2
	exit 2
}

WORK=$(mktemp -d)
trap 'rm -rf -- "${WORK:?}"' EXIT

export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.com
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.com

PASS=0
FAIL=0

expect() {
	if [ "$2" = "$3" ]; then
		PASS=$((PASS + 1))
		echo "ok   $1"
	else
		FAIL=$((FAIL + 1))
		echo "FAIL $1"
		echo "  expected: $2"
		echo "  actual:   $(printf '%s' "$3" | head -c 300)"
	fi
}

commit() { # commit <repo> <subject>
	git -C "$1" -c commit.gpgsign=false commit -q --allow-empty -m "$2"
}

# new_repo <name> <branch> <number of commits>: commits are named c1, c2, and so on.
new_repo() {
	local dir="$WORK/$1" i
	mkdir -p "$dir"
	git -C "$dir" init -q -b "$2" .
	for ((i = 1; i <= $3; i++)); do commit "$dir" "c$i"; done
	REPO_DIR=$dir
}

# run_hook <directory> [stdin]: sets OUT, CODE and ERR. HOOK_PATH replaces PATH when it is set.
run_hook() {
	local dir=$1 input=${2:-}
	OUT=$(
		cd "$dir" || exit 99
		[ -n "${HOOK_PATH:-}" ] && export PATH="$HOOK_PATH"
		printf '%s' "$input" | /bin/bash "$HOOK" 2>"$WORK/err"
	)
	CODE=$?
	ERR=$(cat "$WORK/err")
}

# The context text the hook adds to the session, empty when the hook printed nothing.
context() {
	printf '%s' "$OUT" | jq -r '.hookSpecificOutput.additionalContext' 2>/dev/null
}

# check <label> <expected context>: the context text, exit code 0 and an empty stderr.
check() {
	expect "$1" "$2|0|" "$(context)|$CODE|$ERR"
}

# A repository with three commits
new_repo plain work-branch 3
run_hook "$REPO_DIR"
expect "the output has one top-level key, hookSpecificOutput" \
	hookSpecificOutput "$(printf '%s' "$OUT" | jq -r 'keys | join(",")')"
expect "the output names the SessionStart event" SessionStart \
	"$(printf '%s' "$OUT" | jq -r '.hookSpecificOutput.hookEventName')"
expect "the output holds only the event name and the context" \
	'additionalContext,hookEventName' \
	"$(printf '%s' "$OUT" | jq -r '.hookSpecificOutput | keys | join(",")')"
expect "the exit code is 0 and stderr is empty" "0|" "$CODE|$ERR"
expect "the context has a branch line, a count line, a commits line and one line per commit" 6 \
	"$(context | wc -l | tr -d ' ')"
expect "the first line names the branch" 'Current branch: work-branch' "$(context | sed -n 1p)"
expect "the second line counts the uncommitted files" 'Uncommitted files: 0' "$(context | sed -n 2p)"
expect "the third line opens the commit list" 'Recent commits:' "$(context | sed -n 3p)"
expect "the commits come newest first, each as a short hash and a subject" \
	'c3 c2 c1' \
	"$(context | sed -n '4,$p' | grep -E '^[0-9a-f]{7,} c[0-9]$' | awk '{print $2}' | tr '\n' ' ' | sed 's/ $//')"

# Only the five newest commits
new_repo long main 7
run_hook "$REPO_DIR"
expect "seven commits give five lines in the list" 5 "$(context | sed -n '4,$p' | grep -c .)"
expect "the five are c7 down to c3" 'c7 c6 c5 c4 c3' \
	"$(context | sed -n '4,$p' | awk '{print $2}' | tr '\n' ' ' | sed 's/ $//')"

# Uncommitted files: modified, new, and staged all count, committed files do not
new_repo dirty main 1
printf 'a\n' >"$REPO_DIR/tracked.txt"
git -C "$REPO_DIR" add tracked.txt
commit "$REPO_DIR" 'add tracked'
run_hook "$REPO_DIR"
expect "a clean tree counts 0" 'Uncommitted files: 0' "$(context | sed -n 2p)"
printf 'b\n' >>"$REPO_DIR/tracked.txt"
run_hook "$REPO_DIR"
expect "a modified file counts 1" 'Uncommitted files: 1' "$(context | sed -n 2p)"
printf 'c\n' >"$REPO_DIR/untracked.txt"
run_hook "$REPO_DIR"
expect "a modified and an untracked file count 2" 'Uncommitted files: 2' "$(context | sed -n 2p)"
printf 'd\n' >"$REPO_DIR/staged.txt"
git -C "$REPO_DIR" add staged.txt
run_hook "$REPO_DIR"
expect "a staged file counts too" 'Uncommitted files: 3' "$(context | sed -n 2p)"

# The branch line
git -C "$REPO_DIR" stash -u -q
git -C "$REPO_DIR" checkout -q --detach
run_hook "$REPO_DIR"
expect "a detached HEAD shows as HEAD" 'Current branch: HEAD' "$(context | sed -n 1p)"
git -C "$REPO_DIR" checkout -q -b 'feature/with-slash'
run_hook "$REPO_DIR"
expect "a branch name with a slash is shown whole" 'Current branch: feature/with-slash' "$(context | sed -n 1p)"

# A repository with no commit yet still gives valid output
EMPTY="$WORK/empty-repo"
mkdir -p "$EMPTY"
git -C "$EMPTY" init -q .
run_hook "$EMPTY"
expect "an empty repository still gives a context and exits 0 quietly" \
	'Uncommitted files: 0|0|' "$(context | sed -n 2p)|$CODE|$ERR"
expect "an empty repository lists no commit" 0 "$(context | grep -cE '^[0-9a-f]{7,} ')"

# Subjects with quotes and a backslash come through intact
new_repo quotes main 0
commit "$REPO_DIR" 'say "hi" and a \ backslash'
run_hook "$REPO_DIR"
expect "quotes and a backslash in a commit subject survive in the context" \
	'say "hi" and a \ backslash' "$(context | sed -n 4p | cut -d' ' -f2-)"

# The hook works from a subdirectory and ignores stdin
mkdir -p "$WORK/long/deep/er"
run_hook "$WORK/long/deep/er"
expect "from a subdirectory the hook reports the same repository" 'Current branch: main' "$(context | sed -n 1p)"
run_hook "$WORK/long" ''
FIRST=$OUT
run_hook "$WORK/long" '{"hook_event_name":"SessionStart","source":"startup"}'
expect "the stdin payload does not change the output" "$FIRST" "$OUT"

# Outside a git repository the hook adds nothing
NON_REPO="$WORK/not-a-repo"
mkdir -p "$NON_REPO"
run_hook "$NON_REPO"
expect "outside a repository the hook prints nothing" "|0|" "$OUT|$CODE|$ERR"

# Requires jq and git: without either, the hook adds nothing
mkdir -p "$WORK/empty-path" "$WORK/only-jq" "$WORK/only-git"
ln -s "$(command -v jq)" "$WORK/only-jq/jq"
ln -s "$(type -P git)" "$WORK/only-git/git"
HOOK_PATH="$WORK/empty-path" run_hook "$WORK/long"
expect "without jq the hook prints nothing and exits 0 quietly" "|0|" "$OUT|$CODE|$ERR"
HOOK_PATH="$WORK/only-jq" run_hook "$WORK/long"
expect "without git the hook prints nothing and exits 0 quietly" "|0|" "$OUT|$CODE|$ERR"
HOOK_PATH="$WORK/only-git" run_hook "$WORK/long"
expect "with git but without jq the hook prints nothing and exits 0 quietly" "|0|" "$OUT|$CODE|$ERR"

echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
