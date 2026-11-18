#!/usr/bin/env bash
#
# PostToolUse hook: format the file Claude edited, then lint it.
#
# Formatters:
#   - crates/**/*.rs and apps/**/*.rs: rustfmt on that one file (edition 2021). rustfmt also
#     formats the files a `mod` line names, which are already formatted in a repo that keeps
#     to `cargo fmt`.
#   - apps/ui/**/*.{ts,tsx,js,jsx,mjs,cjs,css}: prettier from apps/ui/node_modules/.bin.
#     Only lines that differ from HEAD are formatted, so editing one function never reflows
#     the rest of the file. A file that is not in HEAD yet is new, so it is formatted whole.
#     A CSS file already in HEAD is skipped; prettier has no partial CSS format.
#
# Never fails the edit. The hook always exits 0. A formatter that is missing or that fails
# is reported to Claude as additionalContext; the edit itself stays as written. The edit has
# already happened when a PostToolUse hook runs, so there is nothing to roll back.
#
# Scope: the path is canonicalized first, so `..` and symlinked directories cannot reach files
# outside the project. A symlinked file, and anything under node_modules, target, dist or
# gen, is skipped.
#
# Trust: prettier and eslint read config and plugins from the checkout and run on every edit.
# That is the same trust level as running the project's tests. Do not enable this hook on a
# checkout you do not trust.
#
# Registered in .claude/settings.json under PostToolUse with matcher "Edit|Write".
# Requires jq. Without jq the hook exits 0 and does nothing. Prettier needs git and node.

set -uo pipefail

command -v jq >/dev/null 2>&1 || exit 0

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT="${CLAUDE_PROJECT_DIR:-$PWD}"

INPUT=$(cat)
FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

if [ -z "$FILE_PATH" ] || [ ! -f "$FILE_PATH" ] || [ -L "$FILE_PATH" ]; then
  exit 0
fi

ROOT=$(cd "$ROOT" 2>/dev/null && pwd -P) || exit 0
FILE_DIR=$(cd "$(dirname -- "$FILE_PATH")" 2>/dev/null && pwd -P) || exit 0
FILE_PATH="$FILE_DIR/$(basename -- "$FILE_PATH")"

NOTES=""

note() {
  NOTES="${NOTES:+$NOTES

}$1"
}

# Print the notes and the linter's own output as one message, then stop. Always exit 0.
finish() {
  local lint_out lint_ctx ctx
  lint_ctx=""
  lint_out=$(printf '%s' "$INPUT" | "$HERE/lint-on-edit.sh" 2>/dev/null) || lint_out=""
  if [ -n "$lint_out" ]; then
    lint_ctx=$(printf '%s' "$lint_out" | jq -r '.hookSpecificOutput.additionalContext // empty' 2>/dev/null)
  fi
  ctx="$NOTES"
  if [ -n "$lint_ctx" ]; then
    ctx="${ctx:+$ctx

}$lint_ctx"
  fi
  if [ -n "$ctx" ]; then
    jq -n --arg ctx "$ctx" '{
      hookSpecificOutput: {
        hookEventName: "PostToolUse",
        additionalContext: $ctx
      }
    }'
  fi
  exit 0
}

KIND=""
TOOL=""
HINT=""

case "$FILE_PATH" in
  */node_modules/* | */target/* | */dist/* | */gen/*)
    exit 0
    ;;
  "$ROOT"/crates/*.rs | "$ROOT"/apps/*.rs)
    KIND=rustfmt
    TOOL=$(command -v rustfmt || true)
    HINT="install rustfmt with 'rustup component add rustfmt'"
    ;;
  "$ROOT"/apps/ui/*.js | "$ROOT"/apps/ui/*.jsx | "$ROOT"/apps/ui/*.ts | "$ROOT"/apps/ui/*.tsx | "$ROOT"/apps/ui/*.mjs | "$ROOT"/apps/ui/*.cjs | "$ROOT"/apps/ui/*.css)
    KIND=prettier
    TOOL="$ROOT/apps/ui/node_modules/.bin/prettier"
    ;;
  *)
    exit 0
    ;;
esac

if [ "$KIND" = rustfmt ]; then
  if [ -z "$TOOL" ] || [ ! -x "$TOOL" ]; then
    note "Not formatted: $FILE_PATH. rustfmt is not on PATH; $HINT."
    finish
  fi
  if ! OUTPUT=$("$TOOL" --edition 2021 -- "$FILE_PATH" 2>&1); then
    note "rustfmt failed for $FILE_PATH. The file is left as written. Fix this first:
$OUTPUT"
  fi
  finish
fi

# Prettier is optional: apps/ui may not have it installed yet. Skip without a message.
[ -x "$TOOL" ] || finish

if ! command -v node >/dev/null 2>&1; then
  note "Not formatted: $FILE_PATH. node is not on PATH."
  finish
fi

format_whole() {
  "$TOOL" --write -- "$FILE_PATH" 2>&1
}

# $1 and $2 are the first and last changed line, 1-based.
format_lines() {
  local offsets
  offsets=$(node -e '
    const lines = require("fs").readFileSync(process.argv[1], "utf8").split("\n");
    const first = +process.argv[2], last = +process.argv[3];
    let at = 0, start = 0, end = 0;
    for (let i = 0; i < last; i++) {
      if (i === first - 1) start = at;
      at += lines[i].length + 1;
      end = at - 1;
    }
    console.log(start, end);
  ' "$FILE_PATH" "$1" "$2") || return 1
  "$TOOL" --write --range-start "${offsets% *}" --range-end "${offsets#* }" -- "$FILE_PATH" 2>&1
}

REL=${FILE_PATH#"$ROOT"/}

if ! command -v git >/dev/null 2>&1 || ! git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
  note "Not formatted: $FILE_PATH. git cannot read the repository at $ROOT, so the changed lines are unknown."
  finish
fi

# A repository with no commit has nothing in HEAD, so every file is new. Any other git failure
# is reported: guessing "new" would reformat a whole tracked file.
IN_HEAD=
if git -C "$ROOT" rev-parse --verify --quiet HEAD >/dev/null; then
  if ! IN_HEAD=$(git -C "$ROOT" ls-tree --name-only HEAD -- "$REL"); then
    note "Not formatted: $FILE_PATH. git ls-tree failed."
    finish
  fi
fi

if [ -z "$IN_HEAD" ]; then
  if ! OUTPUT=$(format_whole); then
    note "prettier failed for $FILE_PATH. The file is left as written:
$OUTPUT"
  fi
  finish
fi

# Prettier cannot format part of a CSS file, so a CSS file already in HEAD is left as written.
case "$FILE_PATH" in
  *.css) finish ;;
esac

# "+start,count" per hunk, last hunk first so earlier line numbers stay valid after a format.
HUNKS=$(git -C "$ROOT" diff --no-color --no-ext-diff --no-textconv -U0 HEAD -- "$REL" | sed -n 's/^@@ [^+]*+\([0-9]*\)\(,\([0-9]*\)\)\{0,1\} @@.*/\1 \3/p' | sort -rn)

while read -r START COUNT; do
  [ -n "$START" ] || continue
  COUNT=${COUNT:-1}
  [ "$COUNT" -eq 0 ] && continue
  if ! OUTPUT=$(format_lines "$START" $((START + COUNT - 1))); then
    note "prettier failed for $FILE_PATH. The file is left as written:
$OUTPUT"
    break
  fi
done <<<"$HUNKS"

finish
