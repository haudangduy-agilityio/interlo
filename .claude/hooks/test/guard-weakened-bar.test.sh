#!/usr/bin/env bash
#
# Tests for guard-weakened-bar.sh and floor-guard.mjs. Each case builds a throwaway git
# repository with one commit, changes it, and runs the hook from inside it with a stop-hook
# payload on stdin. No network, no cargo, no npm.
#
# Every case checks three things together: the decision printed on stdout, the exit code
# (always 0) and that stderr is empty. Claude Code reads the decision from stdout, so a
# decision on stderr or a non-zero exit must fail a case.
#
# Usage: .claude/hooks/test/guard-weakened-bar.test.sh
# Needs jq, git and node.

set -uo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
HOOK="$REPO_ROOT/.claude/hooks/guard-weakened-bar.sh"

for tool in jq git node; do
	command -v "$tool" >/dev/null 2>&1 || {
		echo "$tool is required to run these tests" >&2
		exit 2
	}
done
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

RS_TEST=crates/meeting-core/tests/journal_replay.rs
RS_SRC=crates/meeting-core/src/policy.rs
UI_TEST=apps/ui/src/features/home/Home.test.tsx
UI_SRC=apps/ui/src/features/home/Home.tsx
E2E=apps/ui/e2e/start-meeting.spec.ts

# new_repo <name>: a repository with one commit: a Rust test file with two assertions, a Rust
# source file, a vitest file, a Playwright file, and files outside the checked folders.
# REPO_DIR is left clean.
new_repo() {
	local dir="$WORK/$1"
	mkdir -p "$dir/crates/meeting-core/tests" "$dir/crates/meeting-core/src" \
		"$dir/apps/ui/src/features/home" "$dir/apps/ui/e2e" "$dir/docs" "$dir/scripts"
	git -C "$dir" init -q .
	printf '#[test]\nfn merge_short_pause_without_final_mark() {\n    assert_eq!(merge(1, 2), 3);\n    assert!(merge(0, 0) == 0);\n}\n' >"$dir/$RS_TEST"
	printf 'pub fn merge(a: u32, b: u32) -> u32 {\n    a + b\n}\n' >"$dir/$RS_SRC"
	printf 'it("shows the ready card", () => {\n  expect(true).toBe(true);\n});\n' >"$dir/$UI_TEST"
	printf 'export const Home = () => null;\n' >"$dir/$UI_SRC"
	printf 'test("starts a meeting", async () => {\n  expect(1).toBe(1);\n});\n' >"$dir/$E2E"
	printf 'plain\n' >"$dir/docs/notes.rs"
	printf 'plain\n' >"$dir/scripts/tool.mjs"
	git -C "$dir" add -A
	git -C "$dir" -c commit.gpgsign=false commit -q -m init
	REPO_DIR=$dir
}

# run_hook <stdin>: runs the hook inside REPO_DIR. Sets OUT, CODE and ERR.
run_hook() {
	OUT=$(
		cd "$REPO_DIR" || exit 99
		printf '%s' "$1" | /bin/bash "$HOOK" 2>"$WORK/err"
	)
	CODE=$?
	ERR=$(cat "$WORK/err")
}

decision() {
	if [ -z "$OUT" ]; then
		echo none
	else
		printf '%s' "$OUT" | jq -r '.decision' 2>/dev/null || echo "unreadable: $OUT"
	fi
}

# check <label> <decision>: the last run against a decision, exit code 0 and empty stderr.
check() {
	expect "$1" "$2|0|" "$(decision)|$CODE|$ERR"
}

reason_has() {
	printf '%s' "$OUT" | jq -r '.reason' 2>/dev/null | grep -q -- "$1" && echo yes || echo no
}

INPUT='{"stop_hook_active": false}'

new_repo clean
run_hook "$INPUT"
check "clean tree: no decision" none

# Rust rules
new_repo rs_ignore
printf '\n#[test]\n#[ignore]\nfn slow_case() {}\n' >>"$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "rust: added #[ignore] blocks" block
expect "  reason names the rule and the file" "yes|yes" "$(reason_has 'test-made-easier')|$(reason_has "$RS_TEST")"

new_repo rs_ignore_reason
printf '\n#[test]\n#[ignore = "needs a sound card"]\nfn slow_case() {}\n' >>"$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "rust: added #[ignore = \"reason\"] blocks" block

new_repo rs_allow
printf '#[allow(dead_code)]\nfn unused() {}\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: added #[allow(...)] blocks" block
expect "  reason names silenced-checker" "yes" "$(reason_has 'silenced-checker')"

new_repo rs_inner_allow
printf '#![allow(clippy::all)]\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: added #![allow(...)] blocks" block

new_repo rs_todo
printf 'pub fn later() -> u32 {\n    todo!()\n}\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: added todo!() blocks" block
expect "  reason names unfinished-work" "yes" "$(reason_has 'unfinished-work')"

new_repo rs_todo_msg
printf 'pub fn later() -> u32 {\n    todo!("capture plan")\n}\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: added todo!(\"...\") blocks" block

new_repo rs_unimplemented
printf 'pub fn later() -> u32 {\n    unimplemented!()\n}\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: added unimplemented!() blocks" block

new_repo rs_not_implemented
printf 'pub fn later() -> Result<u32, Error> {\n    Err(Error::NotImplemented)\n}\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: Err(NotImplemented) is the allowed stub and stays clean" none

new_repo rs_skip_iter
printf 'pub fn tail(v: &[u32]) -> Vec<u32> {\n    v.iter().skip(1).copied().collect()\n}\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: Iterator::skip is not a skipped test" none

new_repo rs_lostassert
printf '#[test]\nfn merge_short_pause_without_final_mark() {\n    assert_eq!(merge(1, 2), 3);\n}\n' >"$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "rust: a removed assert! blocks" block
expect "  reason names assertion-removed" "yes" "$(reason_has 'assertion-removed')"

new_repo rs_lostassert_eq
printf '#[test]\nfn merge_short_pause_without_final_mark() {\n    assert!(merge(0, 0) == 0);\n}\n' >"$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "rust: a removed assert_eq! blocks" block

new_repo rs_lostassert_src
printf 'pub fn merge(a: u32, b: u32) -> u32 {\n    assert!(a < 10);\n    a + b\n}\n' >"$REPO_DIR/$RS_SRC"
git -C "$REPO_DIR" -c commit.gpgsign=false commit -q -am "guard"
printf 'pub fn merge(a: u32, b: u32) -> u32 {\n    a + b\n}\n' >"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "rust: a removed assert! in a source file blocks" block

new_repo rs_reworded
printf '#[test]\nfn merge_short_pause_without_final_mark() {\n    assert_eq!(merge(1, 2), 3);\n    assert_ne!(merge(0, 0), 1);\n}\n' >"$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "rust: an assertion replaced one for one stays clean" none

new_repo rs_moreassert
printf '#[test]\nfn merge_short_pause_without_final_mark() {\n    assert_eq!(merge(1, 2), 3);\n    assert!(merge(0, 0) == 0);\n    assert!(merge(2, 2) == 4);\n}\n' >"$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "rust: more assertions than before stays clean" none

new_repo rs_deltest
rm "$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "rust: a deleted tests/ file blocks" block
expect "  reason names test-deleted" "yes" "$(reason_has 'test-deleted')"

new_repo rs_moved
git -C "$REPO_DIR" mv "$RS_TEST" docs/journal_replay.rs
run_hook "$INPUT"
check "rust: a test file moved out of the checked folders blocks" block
expect "  reason names test-deleted" "yes" "$(reason_has 'test-deleted')"

new_repo rs_untracked
printf '#[test]\n#[ignore]\nfn later() {}\n' >"$REPO_DIR/crates/meeting-core/tests/new_case.rs"
run_hook "$INPUT"
check "rust: #[ignore] in a new untracked file blocks" block

# TypeScript and JavaScript rules
new_repo js_skip
sed -i.bak 's/^it(/it.skip(/' "$REPO_DIR/$UI_TEST" && rm "$REPO_DIR/$UI_TEST.bak"
run_hook "$INPUT"
check "ui: it becomes it.skip blocks" block
expect "  reason names test-made-easier" "yes" "$(reason_has 'test-made-easier')"

new_repo js_only
sed -i.bak 's/^it(/it.only(/' "$REPO_DIR/$UI_TEST" && rm "$REPO_DIR/$UI_TEST.bak"
run_hook "$INPUT"
check "ui: it becomes it.only blocks" block

new_repo js_e2e_skip
sed -i.bak 's/^test(/test.skip(/' "$REPO_DIR/$E2E" && rm "$REPO_DIR/$E2E.bak"
run_hook "$INPUT"
check "e2e: test becomes test.skip blocks" block

new_repo js_xit
printf 'xit("later", () => {});\n' >>"$REPO_DIR/$UI_TEST"
run_hook "$INPUT"
check "ui: added xit blocks" block

new_repo js_eslint
printf '// eslint-disable-next-line no-unused-vars\nconst a = 1;\n' >>"$REPO_DIR/$UI_SRC"
run_hook "$INPUT"
check "ui: added eslint-disable blocks" block
expect "  reason names silenced-checker" "yes" "$(reason_has 'silenced-checker')"

new_repo js_tsignore
printf '// @ts-ignore\nconst a: number = "x";\n' >>"$REPO_DIR/$UI_SRC"
run_hook "$INPUT"
check "ui: added @ts-ignore blocks" block

new_repo js_emptycatch
printf 'try { run(); } catch (e) {}\n' >>"$REPO_DIR/$UI_SRC"
run_hook "$INPUT"
check "ui: added empty catch blocks" block
expect "  reason names unfinished-work" "yes" "$(reason_has 'unfinished-work')"

new_repo js_lostassert
printf 'it("shows the ready card", () => {\n});\n' >"$REPO_DIR/$UI_TEST"
run_hook "$INPUT"
check "ui: a removed expect in a kept test file blocks" block
expect "  reason names assertion-removed" "yes" "$(reason_has 'assertion-removed')"

new_repo js_reworded
printf 'it("shows the ready card", () => {\n  expect(false).toBe(false);\n});\n' >"$REPO_DIR/$UI_TEST"
run_hook "$INPUT"
check "ui: an assertion replaced one for one stays clean" none

new_repo js_deltest
rm "$REPO_DIR/$UI_TEST"
run_hook "$INPUT"
check "ui: a deleted .test.tsx file blocks" block

new_repo js_dele2e
rm "$REPO_DIR/$E2E"
run_hook "$INPUT"
check "e2e: a deleted .spec.ts file blocks" block

# Scope and history
new_repo staged
printf '\n#[ignore]\nfn later() {}\n' >>"$REPO_DIR/$RS_TEST"
git -C "$REPO_DIR" add "$RS_TEST"
run_hook "$INPUT"
check "staged but uncommitted #[ignore] blocks" block

new_repo committed
printf '\n#[ignore]\nfn old() {}\n' >>"$REPO_DIR/$RS_TEST"
git -C "$REPO_DIR" -c commit.gpgsign=false commit -q -am "earlier work"
run_hook "$INPUT"
check "an #[ignore] already committed is not this turn's change" none

new_repo outside
printf '#[allow(dead_code)]\nfn a() {}\n' >>"$REPO_DIR/docs/notes.rs"
printf '// eslint-disable-next-line\n' >>"$REPO_DIR/scripts/tool.mjs"
run_hook "$INPUT"
check "a suppression outside crates/, apps/ and fixtures/ is ignored" none

new_repo vendored
mkdir -p "$REPO_DIR/apps/ui/node_modules/pkg" "$REPO_DIR/target/debug" "$REPO_DIR/apps/ui/dist"
printf '// eslint-disable\nit.skip("x", () => {});\n' >"$REPO_DIR/apps/ui/node_modules/pkg/index.js"
printf '#[allow(dead_code)]\n' >"$REPO_DIR/target/debug/build.rs"
printf '// eslint-disable\n' >"$REPO_DIR/apps/ui/dist/app.js"
run_hook "$INPUT"
check "node_modules, target and dist are not checked" none

new_repo tighten
printf '#[allow(dead_code)]\nfn unused() {}\n' >>"$REPO_DIR/$RS_SRC"
git -C "$REPO_DIR" -c commit.gpgsign=false commit -q -am "suppress"
sed -i.bak '/#\[allow(dead_code)\]/d' "$REPO_DIR/$RS_SRC" && rm "$REPO_DIR/$RS_SRC.bak"
run_hook "$INPUT"
check "removing a suppression is silent" none

new_repo active
printf '\n#[ignore]\nfn later() {}\n' >>"$REPO_DIR/$RS_TEST"
run_hook '{"stop_hook_active": true}'
check "stop_hook_active: no decision, avoids the loop" none

NOREPO="$WORK/norepo"
mkdir -p "$NOREPO"
REPO_DIR=$NOREPO
run_hook "$INPUT"
check "outside a git repository: no decision" none

new_repo unicode
printf '#[ignore]\nfn later() {}\n' >"$REPO_DIR/crates/meeting-core/tests/"$'caf\xc3\xa9 case.rs'
run_hook "$INPUT"
check "an #[ignore] in a file with a space and a non-ASCII name blocks" block
expect "  reason names the file unquoted" "yes" "$(reason_has 'café case.rs')"

new_repo bigdiff
awk 'BEGIN { for (i = 0; i < 100000; i++) printf "pub const VALUE_%06d: u32 = 1;\n", i }' >>"$REPO_DIR/$RS_SRC"
printf '#[allow(dead_code)]\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "a diff over 1 MiB does not hide a suppression" block
expect "  reason names the finding, not a guard failure" "yes" "$(reason_has 'silenced-checker')"

new_repo brokenindex
printf 'not an index' >"$REPO_DIR/.git/index"
run_hook "$INPUT"
check "a guard that cannot run inside a repository with commits blocks" block
expect "  reason says the guard could not run" "yes" "$(reason_has 'could not run')"

new_repo attributes
printf '*.rs -diff\n' >"$REPO_DIR/.gitattributes"
git -C "$REPO_DIR" add .gitattributes
git -C "$REPO_DIR" -c commit.gpgsign=false commit -q -m "attributes"
printf '#[allow(dead_code)]\n' >>"$REPO_DIR/$RS_SRC"
run_hook "$INPUT"
check "a -diff attribute does not hide an added suppression" block

new_repo typechange
rm "$REPO_DIR/$RS_TEST"
ln -s ../src/policy.rs "$REPO_DIR/$RS_TEST"
run_hook "$INPUT"
check "a test file replaced by a symlink blocks" block
expect "  reason names test-deleted" "yes" "$(reason_has 'test-deleted')"

# Fail-safe: the guard never prints on stderr and never exits non-zero, with or without node
new_repo nonode
printf '\n#[ignore]\nfn later() {}\n' >>"$REPO_DIR/$RS_TEST"
NO_NODE_DIR=$(mktemp -d)
for t in jq git cat dirname; do ln -s "$(command -v "$t")" "$NO_NODE_DIR/$t"; done
OUT=$(
	cd "$REPO_DIR" || exit 99
	printf '%s' "$INPUT" | PATH="$NO_NODE_DIR" /bin/bash "$HOOK" 2>"$WORK/err"
)
CODE=$?
ERR=$(cat "$WORK/err")
rm -rf "$NO_NODE_DIR"
check "node missing: the hook stays silent and exits 0" none

echo
echo "passed: $PASS, failed: $FAIL"
[ "$FAIL" -eq 0 ]
