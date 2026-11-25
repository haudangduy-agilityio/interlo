#!/usr/bin/env bash
# Run a command with the network denied by the operating system (macOS sandbox).
#   scripts/network-off.sh cargo test -p meeting-core --offline --features test-support egress
# Before the command runs, a canary proves that the sandbox really blocks the
# network. A canary that reaches the network, or a sandbox that cannot run a
# command at all, is NOT RUN (exit 2), never a pass.
# Exit codes: the exit code of the command; 2 not run; 64 bad usage.
# Test hooks (environment): NETWORK_OFF_SANDBOX_EXEC replaces the sandbox tool;
# NETWORK_OFF_CANARY replaces the canary command (run inside the sandbox).
set -u
# The canary is split into words on purpose, but never expanded as a pattern.
set -f

SANDBOX_EXEC="${NETWORK_OFF_SANDBOX_EXEC:-/usr/bin/sandbox-exec}"
PROFILE='(version 1)(allow default)(deny network*)'
DEFAULT_CANARY='/usr/bin/curl -sS --max-time 3 -o /dev/null https://example.com'

if [ -n "${NETWORK_OFF_SANDBOX_EXEC:-}${NETWORK_OFF_CANARY:-}" ]; then
  echo "network-off: test hook in use (NETWORK_OFF_SANDBOX_EXEC or NETWORK_OFF_CANARY)" >&2
fi

if [ "$#" -eq 0 ]; then
  echo "usage: scripts/network-off.sh <command> [args...]" >&2
  exit 64
fi

not_run() {
  echo "network-off: NOT RUN, $1" >&2
  exit 2
}

[ -x "$SANDBOX_EXEC" ] || not_run "sandbox-exec is missing"
"$SANDBOX_EXEC" -p "$PROFILE" /usr/bin/true >/dev/null 2>&1 ||
  not_run "sandbox-exec cannot run a command"

CANARY="${NETWORK_OFF_CANARY:-$DEFAULT_CANARY}"
# The canary is a plain command line; it holds no quotes, so word splitting is
# the intended parsing and `set -f` blocks pattern expansion.
# shellcheck disable=SC2086
if "$SANDBOX_EXEC" -p "$PROFILE" $CANARY >/dev/null 2>&1; then
  not_run "the canary reached the network"
fi
echo "network-off: canary ok, network is denied"

exec "$SANDBOX_EXEC" -p "$PROFILE" "$@"
