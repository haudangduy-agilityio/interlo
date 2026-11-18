#!/usr/bin/env bash
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/guard-env-write.sh"
check() {
  local path="$1" expected="$2" tool="${3:-Edit}" output decision
  output=$(jq -n --arg p "$path" --arg t "$tool" '{tool_name:$t, tool_input:{file_path:$p}}' | "$HOOK")
  decision=allow
  if [ -n "$output" ]; then decision=$(printf '%s' "$output" | jq -r '.hookSpecificOutput.permissionDecision'); fi
  [ "$decision" = "$expected" ] || { echo "FAIL $path: expected $expected, got $decision"; exit 1; }
  echo "ok $path: $decision"
}
for path in .env .env.dev .env.staging .env.prod config/.env.local config/.env.example.local; do check "$path" deny; done
for path in .env.example config/.env.example README.md .environment; do check "$path" allow; done
output=$(printf '%s' '{}' | "$HOOK")
[ "$(printf '%s' "$output" | jq -r '.hookSpecificOutput.permissionDecision')" = deny ]
echo 'ok missing path: deny'
check .env.prod deny Write
check .env.example allow Write
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
ln -s "$tmp/.env.prod" "$tmp/alias"
check "$tmp/alias" deny
ln -s "$tmp/.env.prod" "$tmp/.env.example"
check "$tmp/.env.example" deny
