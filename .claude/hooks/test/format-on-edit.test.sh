#!/usr/bin/env bash
#
# Tests for format-on-edit.sh and lint-on-edit.sh. Each case runs the hook against a
# throwaway git project, so no file in the real repository is touched and nothing is
# installed or downloaded.
#
# Prettier and eslint are stubs written into the throwaway project's node_modules/.bin, so
# the cases prove which tool runs, from where, on which range, and what happens when it is
# missing or fails. They do not prove prettier's own output. rustfmt is the real one; the
# rustfmt cases are reported as SKIP when it is not installed.
#
# Every case checks the exit code (always 0 for these hooks) and, where it matters, that
# stderr is empty and that stdout is valid hook JSON.
#
# Usage: .claude/hooks/test/format-on-edit.test.sh
# Needs jq, git and node.

set -uo pipefail

REAL=$(cd "$(dirname "$0")/../../.." && pwd)
HOOK="$REAL/.claude/hooks/format-on-edit.sh"
LINT="$REAL/.claude/hooks/lint-on-edit.sh"

for tool in jq git node; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "$tool is required to run these tests" >&2
    exit 2
  }
done
[ -x "$HOOK" ] && [ -x "$LINT" ] || {
  echo "hooks are missing or not executable" >&2
  exit 2
}

HAVE_RUSTFMT=0
command -v rustfmt >/dev/null 2>&1 && HAVE_RUSTFMT=1

TMP=$(mktemp -d)
trap 'rm -rf -- "${TMP:?}"' EXIT

export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.com
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.com

PASS=0
FAIL=0
SKIP=0

expect() {
  if [ "$2" = "$3" ]; then
    PASS=$((PASS + 1))
    echo "ok   $1"
  else
    FAIL=$((FAIL + 1))
    echo "FAIL $1"
    echo "  expected: $(printf '%s' "$2" | head -c 300)"
    echo "  actual:   $(printf '%s' "$3" | head -c 300)"
  fi
}

skip() {
  SKIP=$((SKIP + 1))
  echo "SKIP $1"
}

# A prettier stand-in. Without a range it rewrites `{a:1}` as `{ a: 1 }` in the whole file;
# with --range-start and --range-end it rewrites only inside that range, as prettier does.
# It logs its arguments so a case can check what the hook asked for.
write_prettier_stub() {
  mkdir -p "$1/apps/ui/node_modules/.bin"
  cat >"$1/apps/ui/node_modules/.bin/prettier" <<'STUB'
#!/usr/bin/env node
const fs = require('fs');
const a = process.argv.slice(2);
let rs = null, re = null, file = null;
for (let i = 0; i < a.length; i++) {
  if (a[i] === '--range-start') rs = +a[++i];
  else if (a[i] === '--range-end') re = +a[++i];
  else if (a[i] === '--') file = a[++i];
}
if (process.env.STUB_FAIL) {
  console.error('[error] SyntaxError: Unexpected token');
  process.exit(2);
}
let t = fs.readFileSync(file, 'utf8');
const fix = (s) => s.replace(/\{(\w+):(\d+)\}/g, '{ $1: $2 }');
t = rs === null ? fix(t) : t.slice(0, rs) + fix(t.slice(rs, re)) + t.slice(re);
fs.writeFileSync(file, t);
fs.appendFileSync(process.env.STUB_LOG || '/dev/null', (rs === null ? 'whole' : 'range') + ' ' + file + '\n');
STUB
  chmod +x "$1/apps/ui/node_modules/.bin/prettier"
}

# An eslint stand-in. It records the directory it ran from. It prints a finding and exits 1
# when the file holds the word FINDING.
write_eslint_stub() {
  mkdir -p "$1/apps/ui/node_modules/.bin"
  cat >"$1/apps/ui/node_modules/.bin/eslint" <<'STUB'
#!/bin/sh
pwd -P >"$ESLINT_LOG"
for last; do :; done
if grep -q FINDING "$last"; then
  echo "$last: 1:1 error no-unused-vars"
  exit 1
fi
exit 0
STUB
  chmod +x "$1/apps/ui/node_modules/.bin/eslint"
}

make_project() {
  mkdir -p "$1/apps/ui/src" "$1/crates/foo/src" "$1/crates/audio-io/src" \
    "$1/apps/desktop/src-tauri/src" "$1/docs"
}

commit_all() {
  git -C "$1" init -q
  git -C "$1" add -A
  git -C "$1" -c commit.gpgsign=false commit -q -m baseline
}

# Run the format hook for file $2 inside project $1. Sets OUT, ERR and CODE.
run_hook() {
  OUT=$(cd "$1" && printf '{"tool_input":{"file_path":"%s"}}' "$2" | CLAUDE_PROJECT_DIR="$1" "$HOOK" 2>"$TMP/err")
  CODE=$?
  ERR=$(cat "$TMP/err")
}

ctx() {
  printf '%s' "$OUT" | jq -r '.hookSpecificOutput.additionalContext // empty' 2>/dev/null
}

has() {
  case "$(ctx)" in *"$1"*) echo yes ;; *) echo no ;; esac
}

valid_json_or_empty() {
  if [ -z "$OUT" ] || printf '%s' "$OUT" | jq -e '.hookSpecificOutput.hookEventName == "PostToolUse"' >/dev/null 2>&1; then
    echo valid
  else
    echo "invalid: $OUT"
  fi
}

# check_quiet <label>: exit 0, empty stderr, no output
check_quiet() {
  expect "$1" "0||" "$CODE|$ERR|$OUT"
}

# ---------------------------------------------------------------- prettier under apps/ui
P="$TMP/proj"
make_project "$P"
write_prettier_stub "$P"
export STUB_LOG="$TMP/prettier.log"
printf 'const a = {b:1};\nconst c = {d:2};\nconst e = {f:3};\n' >"$P/apps/ui/src/mod.ts"
printf 'a {color:red}\nb {color:blue}\n' >"$P/apps/ui/src/mod.css"
printf 'const a = {b:1};\n' >"$P/apps/ui/src/skipme.ts"
commit_all "$P"

printf 'const a = {b:1};\nconst c = {d:22};\nconst e = {f:3};\n' >"$P/apps/ui/src/mod.ts"
: >"$STUB_LOG"
run_hook "$P" "$P/apps/ui/src/mod.ts"
expect "ui: only the edited line of a tracked file is formatted" \
  $'const a = {b:1};\nconst c = { d: 22 };\nconst e = {f:3};' "$(cat "$P/apps/ui/src/mod.ts")"
expect "ui: the range form of prettier was used, not the whole-file form" "range" "$(cut -d' ' -f1 "$STUB_LOG" | sort -u | tr '\n' ' ' | sed 's/ $//')"
check_quiet "ui: a clean format prints nothing and exits 0"

SUM=$(shasum "$P/apps/ui/src/mod.ts")
run_hook "$P" "$P/apps/ui/src/mod.ts"
expect "ui: a second run changes nothing" "$SUM" "$(shasum "$P/apps/ui/src/mod.ts")"

printf 'const a = {b:11};\nconst c = {d:2};\nconst n = {n:1};\nconst e = {f:3};\nconst g = {h:55};' >"$P/apps/ui/src/mod.ts"
run_hook "$P" "$P/apps/ui/src/mod.ts"
expect "ui: every edited hunk is formatted, untouched lines are not" \
  $'const a = { b: 11 };\nconst c = {d:2};\nconst n = { n: 1 };\nconst e = {f:3};\nconst g = { h: 55 };' "$(cat "$P/apps/ui/src/mod.ts")"

printf 'const x = {y:1};\nconst z = {w:2};\n' >"$P/apps/ui/src/new.tsx"
: >"$STUB_LOG"
run_hook "$P" "$P/apps/ui/src/new.tsx"
expect "ui: a file that is not in HEAD is formatted whole" \
  $'const x = { y: 1 };\nconst z = { w: 2 };' "$(cat "$P/apps/ui/src/new.tsx")"
expect "ui: the whole-file form of prettier was used" "whole" "$(cut -d' ' -f1 "$STUB_LOG" | sort -u | tr '\n' ' ' | sed 's/ $//')"

printf 'a {color:red}\nb {color:purple}\n' >"$P/apps/ui/src/mod.css"
run_hook "$P" "$P/apps/ui/src/mod.css"
expect "css: a tracked file is left as written" $'a {color:red}\nb {color:purple}' "$(cat "$P/apps/ui/src/mod.css")"

printf 'const a = {b:2};\n' >"$P/apps/ui/src/skipme.ts"
STUB_FAIL=1 run_hook "$P" "$P/apps/ui/src/skipme.ts"
expect "ui: a prettier failure exits 0" "0" "$CODE"
expect "ui: a prettier failure leaves the file as written" 'const a = {b:2};' "$(cat "$P/apps/ui/src/skipme.ts")"
expect "ui: a prettier failure is reported to Claude" "yes|valid" "$(has 'prettier failed')|$(valid_json_or_empty)"
expect "ui: a prettier failure prints nothing on stderr" "" "$ERR"

# Vendored and generated files, symlinks and paths outside the project are left alone.
mkdir -p "$P/apps/ui/node_modules/pkg" "$P/apps/ui/dist"
printf 'const a = {b:1};\n' >"$P/apps/ui/node_modules/pkg/index.js"
run_hook "$P" "$P/apps/ui/node_modules/pkg/index.js"
expect "a file under node_modules is left alone" 'const a = {b:1};' "$(cat "$P/apps/ui/node_modules/pkg/index.js")"
check_quiet "a file under node_modules prints nothing"

printf 'const a = {b:1};\n' >"$P/apps/ui/dist/app.js"
run_hook "$P" "$P/apps/ui/dist/app.js"
expect "a file under dist is left alone" 'const a = {b:1};' "$(cat "$P/apps/ui/dist/app.js")"

printf 'const a = {b:1};\n' >"$TMP/target.ts"
ln -s "$TMP/target.ts" "$P/apps/ui/src/link.ts"
run_hook "$P" "$P/apps/ui/src/link.ts"
expect "a symlinked file is left alone" 'const a = {b:1};' "$(cat "$TMP/target.ts")"

mkdir -p "$TMP/elsewhere"
printf 'const a = {b:1};\n' >"$TMP/elsewhere/dir.ts"
ln -s "$TMP/elsewhere" "$P/apps/ui/src/linked"
run_hook "$P" "$P/apps/ui/src/linked/dir.ts"
expect "a file reached through a symlinked directory is left alone" 'const a = {b:1};' "$(cat "$TMP/elsewhere/dir.ts")"

printf 'const a = {b:1};\n' >"$P/outside.ts"
run_hook "$P" "$P/apps/ui/../../outside.ts"
expect "a path that climbs out of apps/ui with .. is left alone" 'const a = {b:1};' "$(cat "$P/outside.ts")"

printf 'plain {a:1}\n' >"$P/docs/notes.md"
run_hook "$P" "$P/docs/notes.md"
expect "a markdown file is left alone" 'plain {a:1}' "$(cat "$P/docs/notes.md")"
check_quiet "a markdown file prints nothing"

run_hook "$P" "$P/does/not/exist.ts"
check_quiet "a path that does not exist exits 0 quietly"

OUT=$(cd "$P" && printf '%s' 'not json' | CLAUDE_PROJECT_DIR="$P" "$HOOK" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
check_quiet "input that is not JSON exits 0 quietly"

OUT=$(cd "$P" && printf '%s' '' | CLAUDE_PROJECT_DIR="$P" "$HOOK" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
check_quiet "empty input exits 0 quietly"

# Prettier not installed yet: nothing happens and nothing is reported.
Q="$TMP/noprettier"
make_project "$Q"
printf 'const a = {b:1};\n' >"$Q/apps/ui/src/a.ts"
commit_all "$Q"
printf 'const a = {b:2};\n' >"$Q/apps/ui/src/a.ts"
run_hook "$Q" "$Q/apps/ui/src/a.ts"
expect "no prettier installed: the file is left as written" 'const a = {b:2};' "$(cat "$Q/apps/ui/src/a.ts")"
check_quiet "no prettier installed: exit 0, no output"

# A git failure is reported, not read as "new file" (which would reformat the whole file).
G="$TMP/nogit"
make_project "$G"
write_prettier_stub "$G"
printf 'const a = {b:1};\nconst c = {d:2};\n' >"$G/apps/ui/src/mod.ts"
run_hook "$G" "$G/apps/ui/src/mod.ts"
expect "outside a git repository: exit 0" "0" "$CODE"
expect "outside a git repository: the file is left alone" $'const a = {b:1};\nconst c = {d:2};' "$(cat "$G/apps/ui/src/mod.ts")"
expect "outside a git repository: Claude is told" "yes|valid" "$(has 'git cannot read')|$(valid_json_or_empty)"

# A repository with no commit has nothing in HEAD, so a file is new.
N="$TMP/nocommit"
make_project "$N"
write_prettier_stub "$N"
git -C "$N" init -q
printf 'const a = {b:1};\n' >"$N/apps/ui/src/mod.ts"
run_hook "$N" "$N/apps/ui/src/mod.ts"
expect "a repository with no commit formats the file whole" 'const a = { b: 1 };' "$(cat "$N/apps/ui/src/mod.ts")"

# ---------------------------------------------------------------- eslint through lint-on-edit
L="$TMP/lint"
make_project "$L"
write_eslint_stub "$L"
export ESLINT_LOG="$TMP/eslint.log"
printf 'export default [];\n' >"$L/apps/ui/eslint.config.js"
printf 'const ok = 1;\n' >"$L/apps/ui/src/ok.ts"
printf 'const bad = 1; // FINDING\n' >"$L/apps/ui/src/bad.ts"
run_lint() {
  OUT=$(cd "$TMP" && printf '{"tool_input":{"file_path":"%s"}}' "$1" | CLAUDE_PROJECT_DIR="$L" "$LINT" 2>"$TMP/err")
  CODE=$?
  ERR=$(cat "$TMP/err")
}

rm -f "$ESLINT_LOG"
run_lint "$L/apps/ui/src/ok.ts"
expect "lint: eslint runs from apps/ui, not from the hook's working directory" \
  "$(cd "$L/apps/ui" && pwd -P)" "$(cat "$ESLINT_LOG" 2>/dev/null)"
check_quiet "lint: a clean file prints nothing"

run_lint "$L/apps/ui/src/bad.ts"
expect "lint: a finding is handed to Claude as hook JSON" "yes|yes|valid|0" "$(has 'Lint failed')|$(has 'no-unused-vars')|$(valid_json_or_empty)|$CODE"

rm -f "$ESLINT_LOG"
printf 'module.exports = {};\n' >"$L/apps/ui/src/x.cjs"
mkdir -p "$L/apps/ui/node_modules/pkg"
printf 'const bad = 1; // FINDING\n' >"$L/apps/ui/node_modules/pkg/index.js"
run_lint "$L/apps/ui/node_modules/pkg/index.js"
expect "lint: a file under node_modules is not linted" "" "$(cat "$ESLINT_LOG" 2>/dev/null)"

printf 'const bad = 1; // FINDING\n' >"$L/outside.ts"
run_lint "$L/outside.ts"
expect "lint: a TypeScript file outside apps/ui is not linted" "" "$(cat "$ESLINT_LOG" 2>/dev/null)"
check_quiet "lint: a file outside apps/ui prints nothing"

M="$TMP/lintnoconfig"
make_project "$M"
write_eslint_stub "$M"
printf 'const bad = 1; // FINDING\n' >"$M/apps/ui/src/bad.ts"
OUT=$(cd "$TMP" && printf '{"tool_input":{"file_path":"%s"}}' "$M/apps/ui/src/bad.ts" | CLAUDE_PROJECT_DIR="$M" "$LINT" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
check_quiet "lint: eslint without a config file is skipped, nothing printed"

K="$TMP/lintnoeslint"
make_project "$K"
printf 'export default [];\n' >"$K/apps/ui/eslint.config.js"
printf 'const bad = 1; // FINDING\n' >"$K/apps/ui/src/bad.ts"
OUT=$(cd "$TMP" && printf '{"tool_input":{"file_path":"%s"}}' "$K/apps/ui/src/bad.ts" | CLAUDE_PROJECT_DIR="$K" "$LINT" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
check_quiet "lint: eslint not installed is skipped, nothing printed"

# The format hook hands the lint finding on, joined with its own notes.
printf 'const bad = {b:1}; // FINDING\n' >"$L/apps/ui/src/both.ts"
write_prettier_stub "$L"
commit_all "$L"
printf 'const bad = {b:2}; // FINDING\n' >"$L/apps/ui/src/both.ts"
run_hook "$L" "$L/apps/ui/src/both.ts"
expect "format then lint: the edited line is formatted" 'const bad = { b: 2 }; // FINDING' "$(cat "$L/apps/ui/src/both.ts")"
expect "format then lint: the eslint finding reaches Claude through the format hook" "yes|valid|0" "$(has 'Lint failed')|$(valid_json_or_empty)|$CODE"

printf 'const bad = {b:3}; // FINDING\n' >"$L/apps/ui/src/both.ts"
STUB_FAIL=1 run_hook "$L" "$L/apps/ui/src/both.ts"
expect "format failure and lint finding are reported together" "yes|yes|valid|0" "$(has 'prettier failed')|$(has 'Lint failed')|$(valid_json_or_empty)|$CODE"

# ---------------------------------------------------------------- Rust reminder and rustfmt
R="$TMP/rust"
make_project "$R"
printf 'pub fn a()->u32{1}\n' >"$R/crates/foo/src/lib.rs"
printf 'pub fn b()->u32{2}\n' >"$R/crates/audio-io/src/lib.rs"
printf 'pub fn c()->u32{3}\n' >"$R/apps/desktop/src-tauri/src/lib.rs"
commit_all "$R"

if [ "$HAVE_RUSTFMT" -eq 1 ]; then
  run_hook "$R" "$R/crates/foo/src/lib.rs"
  expect "rust: the file is formatted with rustfmt" $'pub fn a() -> u32 {\n    1\n}' "$(cat "$R/crates/foo/src/lib.rs")"
  expect "rust: exit 0, stderr empty, valid hook JSON" "0||valid" "$CODE|$ERR|$(valid_json_or_empty)"
  expect "rust: the reminder names the crate's clippy command" "yes" "$(has 'cargo clippy -p foo -- -D warnings')"

  run_hook "$R" "$R/crates/audio-io/src/lib.rs"
  expect "rust: an audio-io edit carries the real-time reminder" "yes|yes" "$(has 'cargo clippy -p audio-io -- -D warnings')|$(has 'never allocate, lock, log or block')"

  run_hook "$R" "$R/apps/desktop/src-tauri/src/lib.rs"
  expect "rust: the desktop shell maps to the interlo-desktop package" "yes" "$(has 'cargo clippy -p interlo-desktop -- -D warnings')"

  printf 'pub fn broken( {\n' >"$R/crates/foo/src/lib.rs"
  run_hook "$R" "$R/crates/foo/src/lib.rs"
  expect "rust: a file rustfmt cannot parse is left as written" 'pub fn broken( {' "$(cat "$R/crates/foo/src/lib.rs")"
  expect "rust: a rustfmt failure exits 0 and is reported" "0|yes|valid" "$CODE|$(has 'rustfmt failed')|$(valid_json_or_empty)"

  printf 'pub fn a()->u32{1}\n' >"$R/crates/foo/src/lib.rs"
  run_hook "$R" "$R/crates/foo/src/lib.rs"
  SUM=$(shasum "$R/crates/foo/src/lib.rs")
  run_hook "$R" "$R/crates/foo/src/lib.rs"
  expect "rust: a second run changes nothing" "$SUM" "$(shasum "$R/crates/foo/src/lib.rs")"
else
  skip "rust: rustfmt is not installed, so the formatting cases did not run"
fi

# A directory that holds only the tools the hook needs, so rustfmt and node are missing.
BARE=$(mktemp -d)
for t in bash jq cat dirname basename git sed sort cut; do
  ln -s "$(command -v "$t")" "$BARE/$t"
done
printf 'pub fn a()->u32{1}\n' >"$R/crates/foo/src/lib.rs"
OUT=$(cd "$R" && printf '{"tool_input":{"file_path":"%s"}}' "$R/crates/foo/src/lib.rs" | CLAUDE_PROJECT_DIR="$R" PATH="$BARE" /bin/bash "$HOOK" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
expect "rust: rustfmt missing leaves the file as written" 'pub fn a()->u32{1}' "$(cat "$R/crates/foo/src/lib.rs")"
expect "rust: rustfmt missing exits 0 and tells Claude, with the reminder" "0||yes|yes|valid" "$CODE|$ERR|$(has 'rustfmt is not on PATH')|$(has 'cargo clippy -p foo')|$(valid_json_or_empty)"

printf 'const a = {b:1};\n' >"$R/apps/ui/src/n.ts"
write_prettier_stub "$R"
OUT=$(cd "$R" && printf '{"tool_input":{"file_path":"%s"}}' "$R/apps/ui/src/n.ts" | CLAUDE_PROJECT_DIR="$R" PATH="$BARE" /bin/bash "$HOOK" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
rm -rf "$BARE"
expect "ui: node missing leaves the file as written and exits 0" "const a = {b:1};|0|yes|valid" "$(cat "$R/apps/ui/src/n.ts")|$CODE|$(has 'node is not on PATH')|$(valid_json_or_empty)"

# Without jq both hooks do nothing and exit 0.
NOJQ=$(mktemp -d)
ln -s "$(command -v cat)" "$NOJQ/cat"
OUT=$(printf '{"tool_input":{"file_path":"%s"}}' "$R/crates/foo/src/lib.rs" | PATH="$NOJQ" /bin/bash "$HOOK" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
check_quiet "format hook without jq: exit 0, no output"
OUT=$(printf '{"tool_input":{"file_path":"%s"}}' "$R/crates/foo/src/lib.rs" | PATH="$NOJQ" /bin/bash "$LINT" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
rm -rf "$NOJQ"
check_quiet "lint hook without jq: exit 0, no output"

echo
echo "$PASS passed, $FAIL failed, $SKIP skipped"
[ "$FAIL" -eq 0 ]
