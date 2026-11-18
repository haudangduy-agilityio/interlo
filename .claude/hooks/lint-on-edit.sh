#!/usr/bin/env bash
#
# PostToolUse hook: lint the file Claude just edited and report failures back.
#
# Contract (https://code.claude.com/docs/en/hooks):
#   - Hook input arrives as JSON on stdin, not as arguments.
#   - The edited path is `.tool_input.file_path`.
#   - Exit 0 and print JSON on stdout to hand Claude the linter output. Printing the
#     failures as `additionalContext` is what makes Claude fix them; a hook that silently
#     swallows linter output teaches Claude nothing.
#
# What it does:
#   - TypeScript and JavaScript under apps/ui: eslint --fix from apps/ui, using the
#     eslint in apps/ui/node_modules. Nothing runs when eslint or its config is missing.
#   - Rust under crates/ and apps/: nothing heavy. clippy needs a whole crate build, so the
#     hook only returns a one-line reminder with the exact command.
#   - Anything else, and anything under node_modules, target, dist or gen: nothing.
#
# Run by format-on-edit.sh after it formats the file. settings.json registers that script
# under PostToolUse with matcher "Edit|Write", not this one.
# Requires jq. Without jq the hook exits 0 and does nothing. It always exits 0.

set -uo pipefail

command -v jq >/dev/null 2>&1 || exit 0

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT="${CLAUDE_PROJECT_DIR:-$(cd "$HERE/../.." && pwd)}"
ROOT=$(cd "$ROOT" 2>/dev/null && pwd -P) || exit 0

INPUT=$(cat)
FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

if [ -z "$FILE_PATH" ] || [ ! -f "$FILE_PATH" ]; then
  exit 0
fi

FILE_DIR=$(cd "$(dirname -- "$FILE_PATH")" 2>/dev/null && pwd -P) || exit 0
FILE_PATH="$FILE_DIR/$(basename -- "$FILE_PATH")"

emit() {
  jq -n --arg ctx "$1" '{
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      additionalContext: $ctx
    }
  }'
  exit 0
}

case "$FILE_PATH" in
  */node_modules/* | */target/* | */dist/* | */gen/*)
    exit 0
    ;;
esac

UI="$ROOT/apps/ui"

case "$FILE_PATH" in
  "$UI"/*.ts | "$UI"/*.tsx | "$UI"/*.js | "$UI"/*.jsx | "$UI"/*.mjs | "$UI"/*.cjs)
    ESLINT="$UI/node_modules/.bin/eslint"
    [ -x "$ESLINT" ] || exit 0
    HAS_CONFIG=0
    for config in "$UI"/eslint.config.* "$UI"/.eslintrc*; do
      [ -e "$config" ] && HAS_CONFIG=1
    done
    [ "$HAS_CONFIG" -eq 1 ] || exit 0

    OUTPUT=$(cd "$UI" && "$ESLINT" --fix -- "$FILE_PATH" 2>&1)
    STATUS=$?

    # The linter passed, or it auto-fixed everything it could. Say nothing.
    if [ "$STATUS" -eq 0 ] || [ -z "$OUTPUT" ]; then
      exit 0
    fi
    emit "Lint failed for $FILE_PATH. Fix these before continuing:
$OUTPUT"
    ;;
  "$ROOT"/crates/*.rs | "$ROOT"/apps/*.rs)
    REL=${FILE_PATH#"$ROOT"/}
    case "$REL" in
      crates/*)
        PACKAGE=${REL#crates/}
        PACKAGE=${PACKAGE%%/*}
        ;;
      apps/desktop/src-tauri/*)
        PACKAGE=interlo-desktop
        ;;
      *)
        PACKAGE=""
        ;;
    esac
    if [ -n "$PACKAGE" ]; then
      CHECK="cargo clippy -p $PACKAGE -- -D warnings"
    else
      CHECK="cargo clippy --workspace -- -D warnings"
    fi
    MESSAGE="Rust file edited: $REL. Before you finish, run: $CHECK"
    case "$REL" in
      crates/audio-io/* | crates/playback/* | crates/engine-host/*)
        MESSAGE="$MESSAGE
Real-time callbacks never allocate, lock, log or block (docs/guardrails.md rule 8)."
        ;;
    esac
    emit "$MESSAGE"
    ;;
esac

exit 0
