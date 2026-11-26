#!/usr/bin/env bash
# Self-test of scripts/check-dev-models.sh. A fake build tool writes the program
# file, so the test compiles nothing. Each rule has a case that must pass and a
# case that must be refused.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The fake build tool: it logs its arguments and writes a program that holds the
# variable name or not, as WITH_FEATURE and WITHOUT_FEATURE say (yes, no, none,
# fail).
cat >"$WORK/cargo" <<'FAKE'
#!/usr/bin/env bash
echo "$*" >>"$LOG"
case "$*" in
  *"--features dev-models"*) kind="$WITH_FEATURE" ;;
  *) kind="$WITHOUT_FEATURE" ;;
esac
case "$kind" in
  yes) printf 'program\0INTERLO_DEV_MODELS_DIR\0end' >"$BIN" ;;
  no) printf 'program\0INTERLO_MODELS\0end' >"$BIN" ;;
  none) rm -f "$BIN" ;;
  fail) exit 101 ;;
esac
FAKE
chmod +x "$WORK/cargo"

FAILURES=0

run_check() { # with without [argument]
  : >"$WORK/log"
  rm -f "$WORK/program"
  STATUS=0
  OUTPUT="$(WITH_FEATURE="$1" WITHOUT_FEATURE="$2" LOG="$WORK/log" BIN="$WORK/program" \
    DEV_MODELS_CARGO="$WORK/cargo" DEV_MODELS_BIN="$WORK/program" \
    bash "$HERE/check-dev-models.sh" "${3:-variable}" 2>&1)" || STATUS=$?
}

expect() { # name status needle
  if [ "$STATUS" = "$2" ] && echo "$OUTPUT" | grep -q -F -- "$3"; then
    echo "ok: $1"
  else
    echo "FAIL: $1: expected exit $2 and \"$3\", got exit $STATUS and \"$OUTPUT\""
    FAILURES=$((FAILURES + 1))
  fi
}

expect_calls() { # name expected-log
  if [ "$(cat "$WORK/log")" = "$2" ]; then
    echo "ok: $1"
  else
    echo "FAIL: $1: expected the calls \"$2\", got \"$(cat "$WORK/log")\""
    FAILURES=$((FAILURES + 1))
  fi
}

run_check yes no
expect "the name only in the developer build passes" 0 "model folder variable check: ok"
expect_calls "each program is built, the default one last" \
  "$(printf '%s\n' 'build -p interlo-desktop --features dev-models' 'build -p interlo-desktop')"

run_check yes yes
expect "the name in the default build is refused" 1 "the default program holds"

run_check no no
expect "a developer build without the name is refused" 1 "the developer program does not hold"

run_check fail no
expect "a failed developer build is refused" 1 "the developer program did not build"

run_check yes fail
expect "a failed default build is refused" 1 "the default program did not build"

run_check none no
expect "a missing program is refused" 1 "no program at"

run_check yes none
expect "a missing default program is refused" 1 "no program at"

run_check yes no nothing
expect "an unknown argument is refused" 64 "usage:"
expect_calls "an unknown argument builds nothing" ""

if [ "$FAILURES" -ne 0 ]; then
  echo "check-dev-models self-test: $FAILURES failed"
  exit 1
fi
echo "check-dev-models self-test: all passed"
