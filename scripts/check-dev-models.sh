#!/usr/bin/env bash
# The gate checks of the desktop program built with the feature `dev-models`.
#   scripts/check-dev-models.sh clippy    lint with the feature
#   scripts/check-dev-models.sh test      test with the feature
#   scripts/check-dev-models.sh variable  build both debug programs and search
#                                         each right after its build: only the
#                                         developer program may hold the name of
#                                         the model folder variable (SL-01)
# Exit codes: 0 passed, 1 failed, 64 bad usage.
set -euo pipefail

NAME="INTERLO_DEV_MODELS_DIR"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The self-test passes its own build tool and program file.
CARGO="${DEV_MODELS_CARGO:-cargo}"
PROGRAM="${DEV_MODELS_BIN:-${CARGO_TARGET_DIR:-$ROOT/target}/debug/interlo-desktop}"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# True when the program holds the name. A search that cannot run is a failure.
holds_name() {
  local status=0
  [ -f "$PROGRAM" ] || fail "no program at $PROGRAM"
  LC_ALL=C grep -a -F -q -- "$NAME" "$PROGRAM" || status=$?
  [ "$status" -le 1 ] || fail "the search of $PROGRAM did not run"
  [ "$status" -eq 0 ]
}

check_variable() {
  "$CARGO" build -p interlo-desktop --features dev-models ||
    fail "the developer program did not build"
  holds_name || fail "the developer program does not hold $NAME"
  # The default program is built last, so it is the one that stays.
  "$CARGO" build -p interlo-desktop || fail "the default program did not build"
  if holds_name; then
    fail "the default program holds $NAME"
  fi
  echo "model folder variable check: ok"
}

cd "$ROOT"
case "${1:-}" in
  clippy) exec "$CARGO" clippy -p interlo-desktop --features dev-models --all-targets -- -D warnings ;;
  test) exec "$CARGO" test -p interlo-desktop --features dev-models ;;
  variable) check_variable ;;
  *)
    echo "usage: scripts/check-dev-models.sh clippy|test|variable" >&2
    exit 64
    ;;
esac
