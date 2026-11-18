#!/usr/bin/env bash
# One gate for local runs and CI.
#   scripts/check.sh quick   format, lint, unit and integration tests, typecheck, GitLab template check
#   scripts/check.sh full    quick, then UI build, desktop build and the hook tests (the gate)
#   scripts/check.sh supply  dependency audit: npm audit (shipped and dev tooling both block; accepted exceptions in scripts/audit-dev.mjs) and cargo deny (install: cargo install cargo-deny --locked)
#   scripts/check.sh e2e     end-to-end journeys only (out of the gate for now)
# Exit codes: 0 all steps passed, 1 a step failed, 2 a step is NOT CONFIGURED,
# 64 bad usage. A missing tool or a missing npm script is never a pass.
set -euo pipefail

MODE="${1:-}"
case "$MODE" in
  quick | full | e2e | supply) ;;
  *)
    echo "usage: scripts/check.sh quick|full|e2e|supply" >&2
    exit 64
    ;;
esac

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UI_DIR="apps/ui"

# Parallel lists, one entry per step that ran (bash 3.2 has no associative arrays).
NAMES=()
RESULTS=()
DURATIONS=()
TOTAL_START=$(date +%s)

print_summary() {
  local i
  echo
  echo "== Summary ($MODE) =="
  printf '%-52s %-16s %s\n' "STEP" "RESULT" "SECONDS"
  printf '%-52s %-16s %s\n' "----" "------" "-------"
  for ((i = 0; i < ${#NAMES[@]}; i++)); do
    printf '%-52s %-16s %s\n' "${NAMES[$i]}" "${RESULTS[$i]}" "${DURATIONS[$i]}"
  done
  printf 'Total: %ss\n' "$(($(date +%s) - TOTAL_START))"
}

record() { # name result seconds
  NAMES+=("$1")
  RESULTS+=("$2")
  DURATIONS+=("$3")
}

have_cmd() { command -v "$1" >/dev/null 2>&1; }

# True when apps/ui/package.json defines the named npm script.
has_npm_script() {
  have_cmd node || return 1
  node -e '
    const s = require("./'"$UI_DIR"'/package.json").scripts || {};
    process.exit(Object.prototype.hasOwnProperty.call(s, process.argv[1]) ? 0 : 1);
  ' "$1" 2>/dev/null
}

not_configured() { # name reason
  echo "NOT CONFIGURED: $1"
  echo "  reason: $2"
  record "$1" "NOT CONFIGURED" "0"
  print_summary
  exit 2
}

# run_step <name> <requirement> <command...>
# requirement: "cargo", "cargo-fmt", "cargo-clippy", or "npm:<script>" (empty for none).
run_step() {
  local name="$1" req="$2"
  shift 2

  echo
  echo "==> $name"
  echo "    \$ $*"

  case "$req" in
    cargo)
      have_cmd cargo || not_configured "$name" "cargo is not installed"
      ;;
    cargo-fmt)
      have_cmd cargo || not_configured "$name" "cargo is not installed"
      cargo fmt --version >/dev/null 2>&1 || not_configured "$name" "rustfmt component is missing (rustup component add rustfmt)"
      ;;
    cargo-clippy)
      have_cmd cargo || not_configured "$name" "cargo is not installed"
      cargo clippy --version >/dev/null 2>&1 || not_configured "$name" "clippy component is missing (rustup component add clippy)"
      ;;
    npm:*)
      local script="${req#npm:}"
      have_cmd node || not_configured "$name" "node is not installed"
      have_cmd npm || not_configured "$name" "npm is not installed"
      [ -d "$UI_DIR/node_modules" ] || not_configured "$name" "$UI_DIR/node_modules is missing (run: npm --prefix $UI_DIR ci)"
      if [ "$script" != "-" ]; then
        has_npm_script "$script" || not_configured "$name" "npm script \"$script\" is not defined in $UI_DIR/package.json"
      fi
      ;;
    node)
      have_cmd node || not_configured "$name" "node is not installed"
      ;;
    jq)
      have_cmd jq || not_configured "$name" "jq is not installed (the hook tests build their input with it)"
      have_cmd python3 || not_configured "$name" "python3 is not installed (the hook tests use it)"
      ;;
    cargo-deny)
      have_cmd cargo || not_configured "$name" "cargo is not installed"
      cargo deny --version >/dev/null 2>&1 || not_configured "$name" "cargo-deny is not installed (cargo install cargo-deny)"
      [ -f deny.toml ] || not_configured "$name" "deny.toml is missing from the repository root"
      ;;
    "") ;;
    *)
      echo "internal error: unknown requirement '$req'" >&2
      exit 70
      ;;
  esac

  local start status=0
  start=$(date +%s)
  "$@" || status=$?
  local secs=$(($(date +%s) - start))

  if [ "$status" -ne 0 ]; then
    echo "    -> FAIL after ${secs}s (exit $status)"
    record "$name" "FAIL" "$secs"
    echo
    echo "FAILED STEP: $name"
    print_summary
    exit 1
  fi

  echo "    -> PASS in ${secs}s"
  record "$name" "PASS" "$secs"
}

run_quick() {
  run_step "cargo fmt --check" cargo-fmt cargo fmt --all --check
  run_step "cargo clippy" cargo-clippy cargo clippy --workspace --all-targets -- -D warnings
  run_step "cargo test" cargo cargo test --workspace
  run_step "ui lint" "npm:lint" npm --prefix "$UI_DIR" run lint
  run_step "ui format:check" "npm:format:check" npm --prefix "$UI_DIR" run format:check
  run_step "ui typecheck" "npm:typecheck" npm --prefix "$UI_DIR" run typecheck
  run_step "ui test" "npm:test" npm --prefix "$UI_DIR" test
  run_step "gitlab templates" node node scripts/check-templates.mjs
  run_step "gitlab templates self-test" node node --test scripts/check-templates.test.mjs
}

# The hook tests guard the agent safety net. A failing suite prints its output.
run_hook_tests() {
  local t out status=0
  for t in .claude/hooks/test/*.test.sh; do
    out=$(bash "$t" 2>&1) || {
      echo "--- FAILED: $t"
      echo "$out"
      status=1
    }
  done
  return "$status"
}

run_full_extra() {
  run_step "ui build" "npm:build" npm --prefix "$UI_DIR" run build
  run_step "cargo build -p interlo-desktop" cargo cargo build -p interlo-desktop
  run_step "hook tests" jq run_hook_tests
}

run_supply() {
  run_step "npm audit, shipped (high)" "npm:-" npm --prefix "$UI_DIR" audit --omit=dev --audit-level=high
  run_step "npm audit, dev tooling (high, see scripts/audit-dev.mjs)" "npm:-" node scripts/audit-dev.mjs "$UI_DIR"
  run_step "cargo deny check" cargo-deny cargo deny check
}

if [ "$MODE" = "e2e" ]; then
  run_step "ui e2e" "npm:e2e" npm --prefix "$UI_DIR" run e2e
elif [ "$MODE" = "supply" ]; then
  run_supply
else
  run_quick
  if [ "$MODE" = "full" ]; then
    run_full_extra
  fi
fi

print_summary
echo
echo "ALL STEPS PASSED ($MODE)"
