#!/usr/bin/env bash
# Block direct Edit/Write calls to environment files except the exact example name.
set -uo pipefail
INPUT=$(cat)
if command -v python3 >/dev/null 2>&1; then
  if ! printf '%s' "$INPUT" | python3 -c '
import json, os, sys
try:
    data = json.load(sys.stdin)
    path = data["tool_input"]["file_path"]
    if not isinstance(path, str) or not path:
        raise ValueError("missing file path")
    names = (os.path.basename(os.path.normpath(path)), os.path.basename(os.path.realpath(path)))
    denied = any((name == ".env" or name.startswith(".env.")) and name != ".env.example" for name in names)
except Exception:
    denied = True
if denied:
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "Environment files are read-only; only .env.example may be edited."}}))
'; then
    printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Environment file protection failed."}}'
  fi
else
  printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"python3 is required to check environment file protection."}}'
fi
