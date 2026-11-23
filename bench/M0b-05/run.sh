#!/usr/bin/env bash
# Start one run of the spike through its signed bundle and sample its sockets (AC-39).
#
# Usage: bench/M0b-05/run.sh <run-name> [spike options...]
#
# The spike starts through `open`, so the audio-capture permission attaches to the bundle and
# not to the terminal. The script records the pid, then samples `lsof -nP -a -p <pid> -i`
# every second into $TMPDIR/m0b-05/lsof.log until the spike exits, and returns at once; the
# sampler keeps running in the background. Files of the run, in $TMPDIR/m0b-05/runs/<run-name>/:
#   spike.log  stdout of the spike (one `<epoch ms> <event>` line per event)
#   spike.err  stderr of the spike
#   pid        pid of the spike
#   exit       exit code, written by the spike just before it exits
#   lsof.log   copy of the socket samples of this run
#   stdin.fifo send a line to the spike with: printf '%s\n' "mark name" > stdin.fifo
#              (an empty line logs SOURCE_END)
set -euo pipefail

NAME="${1:?usage: run.sh <run-name> [spike options...]}"
shift
case "$NAME" in
  '' | . | .. | *[!A-Za-z0-9._-]*) echo "run.sh: the run name may hold only letters, digits, dot, dash and underscore" >&2; exit 2 ;;
esac
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="${M0B05_APP:-$HERE/target/bundle/m0b05.app}"
BASE="${TMPDIR:-/tmp}"
BASE="${BASE%/}/m0b-05"
RUN="$BASE/runs/$NAME"

[ -d "$APP" ] || { echo "run.sh: bundle not found: $APP (build it with bench/M0b-05/bundle.sh)" >&2; exit 2; }
mkdir -p "$RUN"
rm -f "$RUN/spike.log" "$RUN/spike.err" "$RUN/pid" "$RUN/exit" "$RUN/lsof.log" "$RUN/stdin.fifo"
mkfifo "$RUN/stdin.fifo"
: > "$BASE/lsof.log"

# The sampler holds the write end of the fifo open for the whole run, so the spike never
# sees end-of-file on its input before it exits.
(
  exec 3<>"$RUN/stdin.fifo"
  open -n --stdin "$RUN/stdin.fifo" --stdout "$RUN/spike.log" --stderr "$RUN/spike.err" \
    -a "$APP" --args --pidfile "$RUN/pid" --exitfile "$RUN/exit" "$@"
  tries=0
  until [ -s "$RUN/pid" ] || [ "$tries" -ge 100 ]; do tries=$((tries + 1)); sleep 0.1; done
  PID="$(cat "$RUN/pid" 2>/dev/null || true)"
  case "$PID" in '' | *[!0-9]*) PID="" ;; esac
  if [ -n "$PID" ]; then
    # One sample at once, then one per second while the spike lives. A run that ends in less than
    # a second can only be sampled once: a limit of the method.
    while :; do
      {
        echo "# sample $(python3 -c 'import time; print(int(time.time()*1000))')"
        lsof_exit=0
        lsof -nP -a -p "$PID" -i 2>/dev/null || lsof_exit=$?
        echo "# lsof exit=$lsof_exit"
      } >> "$BASE/lsof.log"
      kill -0 "$PID" 2>/dev/null || break
      sleep 1
    done
  fi
  cp "$BASE/lsof.log" "$RUN/lsof.log"
) > /dev/null 2>&1 &

tries=0
until [ -s "$RUN/pid" ] || [ "$tries" -ge 100 ]; do tries=$((tries + 1)); sleep 0.1; done
[ -s "$RUN/pid" ] || { echo "run.sh: the spike did not start" >&2; exit 3; }
echo "run=$NAME pid=$(cat "$RUN/pid") dir=$RUN"
