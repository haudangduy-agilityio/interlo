#!/usr/bin/env bash
#
# Tests for lint-on-edit.sh. Each case feeds the hook one synthetic edit on stdin, inside a
# throwaway project directory. eslint is a stub, so nothing is installed and nothing touches
# the network. The Rust cases need no rustfmt: the hook only prints a reminder for Rust.
#
# Every case checks the exit code (always 0), that stderr is empty, and that stdout is
# either empty or a PostToolUse hook JSON.
#
# Usage: .claude/hooks/test/lint-on-edit.test.sh
# Needs jq.

set -uo pipefail

REAL=$(cd "$(dirname "$0")/../../.." && pwd)
LINT="$REAL/.claude/hooks/lint-on-edit.sh"

command -v jq >/dev/null 2>&1 || {
  echo "jq is required to read the hook's output" >&2
  exit 2
}
[ -x "$LINT" ] || {
  echo "hook is missing or not executable: $LINT" >&2
  exit 2
}

TMP=$(mktemp -d)
trap 'rm -rf -- "${TMP:?}"' EXIT

PASS=0
FAIL=0

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

# An eslint stand-in. It records the directory and the arguments it ran with. It prints a
# finding and exits 1 when the file holds the word FINDING.
write_eslint_stub() {
  mkdir -p "$1/apps/ui/node_modules/.bin"
  cat >"$1/apps/ui/node_modules/.bin/eslint" <<'STUB'
#!/bin/sh
{ pwd -P; echo "$@"; } >"$ESLINT_LOG"
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
    "$1/crates/playback/src" "$1/crates/engine-host/src" "$1/crates/store/src" \
    "$1/apps/desktop/src-tauri/src" "$1/docs"
}

# run_lint <project> <file>: sets OUT, ERR and CODE. The hook runs from an unrelated directory.
run_lint() {
  OUT=$(cd "$TMP" && printf '{"tool_input":{"file_path":"%s"}}' "$2" | CLAUDE_PROJECT_DIR="$1" "$LINT" 2>"$TMP/err")
  CODE=$?
  ERR=$(cat "$TMP/err")
}

ctx() {
  printf '%s' "$OUT" | jq -r '.hookSpecificOutput.additionalContext // empty' 2>/dev/null
}

has() {
  case "$(ctx)" in *"$1"*) echo yes ;; *) echo no ;; esac
}

valid() {
  if [ -z "$OUT" ] || printf '%s' "$OUT" | jq -e '.hookSpecificOutput.hookEventName == "PostToolUse"' >/dev/null 2>&1; then
    echo valid
  else
    echo "invalid: $OUT"
  fi
}

quiet() {
  expect "$1" "0||" "$CODE|$ERR|$OUT"
}

export ESLINT_LOG="$TMP/eslint.log"

# ---------------------------------------------------------------- eslint under apps/ui
L="$TMP/proj"
make_project "$L"
write_eslint_stub "$L"
printf 'export default [];\n' >"$L/apps/ui/eslint.config.js"
printf 'const ok = 1;\n' >"$L/apps/ui/src/ok.ts"
printf 'const bad = 1; // FINDING\n' >"$L/apps/ui/src/bad.tsx"

rm -f "$ESLINT_LOG"
run_lint "$L" "$L/apps/ui/src/ok.ts"
expect "eslint runs from apps/ui, not from the hook's working directory" \
  "$(cd "$L/apps/ui" && pwd -P)" "$(head -1 "$ESLINT_LOG" 2>/dev/null)"
expect "eslint is asked to fix and gets the file after --" "yes" \
  "$(tail -n +2 "$ESLINT_LOG" | grep -q -- "--fix -- $(cd "$L/apps/ui/src" && pwd -P)/ok.ts" && echo yes || echo no)"
quiet "a clean file prints nothing"

run_lint "$L" "$L/apps/ui/src/bad.tsx"
expect "a finding is handed to Claude as hook JSON, exit 0" "yes|yes|valid|0|" \
  "$(has 'Lint failed')|$(has 'no-unused-vars')|$(valid)|$CODE|$ERR"

for ext in ts tsx js jsx mjs cjs; do
  rm -f "$ESLINT_LOG"
  printf 'const a = 1;\n' >"$L/apps/ui/src/file.$ext"
  run_lint "$L" "$L/apps/ui/src/file.$ext"
  expect "an .$ext file under apps/ui is linted" "yes" "$([ -s "$ESLINT_LOG" ] && echo yes || echo no)"
done

rm -f "$ESLINT_LOG"
printf 'body { color: red }\n' >"$L/apps/ui/src/a.css"
run_lint "$L" "$L/apps/ui/src/a.css"
expect "a CSS file is not linted" "no" "$([ -s "$ESLINT_LOG" ] && echo yes || echo no)"

rm -f "$ESLINT_LOG"
mkdir -p "$L/apps/ui/node_modules/pkg" "$L/apps/ui/dist"
printf 'const bad = 1; // FINDING\n' >"$L/apps/ui/node_modules/pkg/index.js"
printf 'const bad = 1; // FINDING\n' >"$L/apps/ui/dist/app.js"
run_lint "$L" "$L/apps/ui/node_modules/pkg/index.js"
run_lint "$L" "$L/apps/ui/dist/app.js"
expect "files under node_modules and dist are not linted" "no" "$([ -s "$ESLINT_LOG" ] && echo yes || echo no)"
quiet "files under dist print nothing"

rm -f "$ESLINT_LOG"
printf 'const bad = 1; // FINDING\n' >"$L/outside.ts"
run_lint "$L" "$L/outside.ts"
expect "a TypeScript file outside apps/ui is not linted" "no" "$([ -s "$ESLINT_LOG" ] && echo yes || echo no)"
quiet "a file outside apps/ui prints nothing"

# eslint missing or without a config: skipped quietly
M="$TMP/noconfig"
make_project "$M"
write_eslint_stub "$M"
printf 'const bad = 1; // FINDING\n' >"$M/apps/ui/src/bad.ts"
rm -f "$ESLINT_LOG"
run_lint "$M" "$M/apps/ui/src/bad.ts"
quiet "eslint without a config file is skipped, nothing printed"
expect "eslint without a config file never ran" "no" "$([ -s "$ESLINT_LOG" ] && echo yes || echo no)"

K="$TMP/noeslint"
make_project "$K"
printf 'export default [];\n' >"$K/apps/ui/eslint.config.js"
printf 'const bad = 1; // FINDING\n' >"$K/apps/ui/src/bad.ts"
run_lint "$K" "$K/apps/ui/src/bad.ts"
quiet "eslint not installed is skipped, nothing printed"

# A legacy config name counts as a config
Y="$TMP/legacy"
make_project "$Y"
write_eslint_stub "$Y"
printf '{}\n' >"$Y/apps/ui/.eslintrc.json"
printf 'const bad = 1; // FINDING\n' >"$Y/apps/ui/src/bad.ts"
run_lint "$Y" "$Y/apps/ui/src/bad.ts"
expect "an .eslintrc.json counts as a config" "yes" "$(has 'Lint failed')"

# ---------------------------------------------------------------- Rust: a reminder only
R="$TMP/rust"
make_project "$R"
for f in crates/foo/src/lib.rs crates/audio-io/src/lib.rs crates/playback/src/lib.rs \
  crates/engine-host/src/lib.rs crates/store/src/lib.rs apps/desktop/src-tauri/src/lib.rs; do
  printf 'pub fn a() {}\n' >"$R/$f"
done
printf 'pub fn a() {}\n' >"$R/docs/notes.rs"

run_lint "$R" "$R/crates/foo/src/lib.rs"
expect "rust: the reminder names the crate's clippy command" "yes|valid|0|" \
  "$(has 'cargo clippy -p foo -- -D warnings')|$(valid)|$CODE|$ERR"
expect "rust: a plain crate gets no real-time reminder" "no" "$(has 'never allocate')"

for crate in audio-io playback engine-host; do
  run_lint "$R" "$R/crates/$crate/src/lib.rs"
  expect "rust: a $crate edit carries the real-time reminder" "yes|yes" \
    "$(has "cargo clippy -p $crate -- -D warnings")|$(has 'never allocate, lock, log or block')"
done

run_lint "$R" "$R/crates/store/src/lib.rs"
expect "rust: a store edit has no real-time reminder" "yes|no" \
  "$(has 'cargo clippy -p store -- -D warnings')|$(has 'never allocate')"

run_lint "$R" "$R/apps/desktop/src-tauri/src/lib.rs"
expect "rust: the desktop shell maps to the interlo-desktop package" "yes" \
  "$(has 'cargo clippy -p interlo-desktop -- -D warnings')"

run_lint "$R" "$R/docs/notes.rs"
quiet "rust: a .rs file outside crates/ and apps/ prints nothing"

mkdir -p "$R/crates/foo/target/debug"
printf 'fn a() {}\n' >"$R/crates/foo/target/debug/build.rs"
run_lint "$R" "$R/crates/foo/target/debug/build.rs"
quiet "rust: a file under target prints nothing"

# ---------------------------------------------------------------- inputs that carry nothing
run_lint "$R" "$R/crates/foo/src/missing.rs"
quiet "a path that does not exist prints nothing"
run_lint "$R" "$R/docs"
quiet "a directory prints nothing"
printf 'plain\n' >"$R/docs/a.md"
run_lint "$R" "$R/docs/a.md"
quiet "a markdown file prints nothing"
OUT=$(cd "$TMP" && printf '%s' 'not json' | CLAUDE_PROJECT_DIR="$R" "$LINT" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
quiet "input that is not JSON prints nothing and exits 0"
OUT=$(cd "$TMP" && printf '%s' '' | CLAUDE_PROJECT_DIR="$R" "$LINT" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
quiet "empty input prints nothing and exits 0"

# A path with `..` that climbs out of apps/ui is canonicalized first
rm -f "$ESLINT_LOG"
run_lint "$L" "$L/apps/ui/../../outside.ts"
expect "a path that climbs out of apps/ui with .. is not linted" "no" "$([ -s "$ESLINT_LOG" ] && echo yes || echo no)"

# Without jq the hook does nothing and exits 0
NOJQ=$(mktemp -d)
ln -s "$(command -v cat)" "$NOJQ/cat"
OUT=$(printf '{"tool_input":{"file_path":"%s"}}' "$R/crates/foo/src/lib.rs" | PATH="$NOJQ" /bin/bash "$LINT" 2>"$TMP/err")
CODE=$?
ERR=$(cat "$TMP/err")
rm -rf "$NOJQ"
quiet "without jq: exit 0, no output"

echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
