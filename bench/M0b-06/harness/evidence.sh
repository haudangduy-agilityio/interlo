#!/usr/bin/env bash
# Evidence runs of M0b-06.
#
# Usage:
#   evidence.sh controls <evidence-dir>   checks that need no person (control of AC-23, AC-31)
#   evidence.sh sitting  <evidence-dir>   the sitting with the owner: a full-screen test window,
#                                         then both window mechanisms in turn
#   M0B06_BROWSER=<path of a browser app that accepts --user-data-dir>   (sitting only)
#   M0B06_SIMULATE=1   sitting without a browser and without a person (fake_page.py answers):
#                      only for a dry run of this script; its results are not evidence
#   M0B06_SHOTS=<folder outside git for the screenshots>
#
# The screenshots stay outside git (owner rule 2026-11-23). The text logs are copied into the
# evidence directory with the home directory and the temporary directory replaced.
# The script sends no key or pointer event. Every key press and click is the owner's.
set -euo pipefail

MODE="${1:?usage: evidence.sh controls|sitting <evidence-dir>}"
EVID="${2:?usage: evidence.sh controls|sitting <evidence-dir>}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH="$(cd "$HERE/.." && pwd)"
BIN="$BENCH/target/bundle/m0b06.app/Contents/MacOS/m0b06"
BASE="${TMPDIR:-/tmp}"
BASE="${BASE%/}/m0b-06"
RUNS="$BASE/runs"
SHOTS="${M0B06_SHOTS:-$HOME/Desktop/workspace/training/ai/interlo-evidence/M0b-06}"
PANEL_HEX=E6007E
PAGE_HEX=0B3D91
STATE="$BASE/state.json"
PAGELOG="$BASE/page.log"
PROF="$BASE/profile"
SEQ=0

mkdir -p "$EVID" "$SHOTS" "$RUNS"
EVID="$(cd "$EVID" && pwd)"

now_ms() { python3 -c 'import time; print(int(time.time()*1000))'; }
sleep_until() { local t="$1"; while [ "$(now_ms)" -lt "$t" ]; do sleep 0.05; done; }
res() { printf '%s\n' "$*" | tee -a "$EVID/results.txt"; }
hlog() { printf '%s %s\n' "$(now_ms)" "$*" | tee -a "$EVID/harness.log"; }
sendcmd() { printf '%s\n' "$2" > "$RUNS/$1/stdin.fifo"; }
run_log() { echo "$RUNS/$1/spike.log"; }
last_ts() { awk -v e="$2" '$2==e{t=$1} END{print t+0}' "$1"; }
count_ev() { awk -v e="$2" '$2==e{n++} END{print n+0}' "$1"; }
wait_for() { # timeout-seconds command...
  local end=$(( $(date +%s) + $1 )); shift
  while ! "$@"; do [ "$(date +%s)" -ge "$end" ] && return 1; sleep 0.25; done
}
has_targets() { [ "$(count_ev "$PAGELOG" TARGET)" -ge "$1" ]; }
analyze() { "$BIN" analyze "$@" || true; }

state() { # step text [x y]
  SEQ=$((SEQ + 1))
  python3 - "$SEQ" "$@" > "$STATE.tmp" <<'PY'
import json, sys
seq, step, text = sys.argv[1:4]
st = {"seq": int(seq), "step": step, "text": text}
if len(sys.argv) >= 6:
    st["target"] = {"x": int(float(sys.argv[4])), "y": int(float(sys.argv[5])), "size": 80}
print(json.dumps(st))
PY
  mv "$STATE.tmp" "$STATE"
  hlog "STATE $2"
}

shot() { # name mechanism -> prints "<start> <end> <file>"
  local file="$SHOTS/$2-$1.png" s e
  s="$(now_ms)"
  screencapture -x -D 1 "$file" 2>/dev/null || true
  e="$(now_ms)"
  echo "$s SHOT $2-$1 start=$s end=$e" >> "$EVID/shots.log"
  echo "$s $e $file"
}

shot_check() { # mode file rect(4) ; prints the line and returns its verdict word
  # The screenshot carries the colour profile of the display, so its raw values differ from the
  # nominal ones (a nominal #E6007E read (203,44,111) in a dry run). The check reads a copy converted
  # to sRGB; the original file is the one that is hashed and shown.
  local src="$2" conv="$SHOTS/srgb/$(basename "$2")"
  mkdir -p "$SHOTS/srgb"
  if sips --matchTo "/System/Library/ColorSync/Profiles/sRGB Profile.icc" "$src" --out "$conv" > /dev/null 2>&1; then
    src="$conv"
  fi
  python3 -I "$HERE/shot_check.py" "$1" "$src" "$3" "$4" "$5" "$6" "$PANEL_HEX" "$PAGE_HEX" || true
}

start_server() {
  : > "$PAGELOG"
  rm -f "$STATE" "$BASE/port"
  python3 -I "$HERE/serve.py" "$STATE" "$PAGELOG" "$BASE/port" > /dev/null 2>&1 &
  SERVER_PID=$!
  wait_for 10 test -s "$BASE/port"
  PORT="$(cat "$BASE/port")"
  hlog "SERVER port=$PORT pid=$SERVER_PID"
}

cleanup() {
  set +e
  for r in sit-flags sit-native control-ac23; do
    if [ -s "$RUNS/$r/pid" ] && kill -0 "$(cat "$RUNS/$r/pid")" 2>/dev/null; then
      ( printf 'quit\n' > "$RUNS/$r/stdin.fifo" ) > /dev/null 2>&1 &
    fi
  done
  sleep 1
  [ -n "${FAKE_PID:-}" ] && kill "$FAKE_PID" 2>/dev/null
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null
  if [ -d "$PROF" ]; then
    ps -axww -o pid=,command= | grep -F -- "--user-data-dir=$PROF" | grep -v grep | awk '{print $1}' | xargs kill 2>/dev/null
    sleep 1
    rm -rf -- "$PROF"
  fi
}
trap cleanup EXIT

# The bundle id of any other frontmost application (the test browser, or another app) is replaced: a product
# name does not go into the repository. The bundle id of the spike itself is kept.
sanitize() {
  sed -e "s#$HOME#~#g" -e "s#${TMPDIR:-/tmp}#\$TMPDIR#g" \
    -e 's#front_bundle=app\.interlo\.bench\.m0b06#SELFBUNDLE#g' \
    -e 's#front_bundle=[^ ]*#front_bundle=other.app#g' \
    -e 's#SELFBUNDLE#front_bundle=app.interlo.bench.m0b06#g'
}

collect() { # run-name label
  sanitize < "$RUNS/$1/spike.log" > "$EVID/$2-spike.log"
  [ -f "$RUNS/$1/lsof.log" ] && sanitize < "$RUNS/$1/lsof.log" > "$EVID/$2-lsof.log"
  [ -s "$RUNS/$1/spike.err" ] && sanitize < "$RUNS/$1/spike.err" > "$EVID/$2-spike.err"
  return 0
}

start_spike() { # run-name [spike options...]
  local name="$1"; shift
  "$BENCH/run.sh" "$name" "$@" > /dev/null
  wait_for 30 grep -q READY "$(run_log "$name")" || { hlog "ERROR $name did not print READY"; exit 3; }
}

hashes() {
  (cd "$EVID" && find . -maxdepth 1 -type f ! -name SHA256SUMS ! -name SHOT-SHA256SUMS | sort | xargs shasum -a 256 > SHA256SUMS)
  (cd "$SHOTS" && ls ./*.png > /dev/null 2>&1 && shasum -a 256 ./*.png > "$EVID/SHOT-SHA256SUMS") || true
}

do_controls() {
  : > "$EVID/results.txt"; : > "$EVID/harness.log"
  hlog "CONTROLS start; macOS $(sw_vers -productVersion); displays=$(system_profiler SPDisplaysDataType | grep -c Resolution)"
  # AC-23 control: the check must be able to say NO. A real spike log is copied and its `after`
  # snapshot is given another frontmost pid; the check must say NO on the copy. This shows only that
  # the check can say NO; that it says PASS when it should is covered by the unit tests and the PASS
  # lines of the sittings (a PASS on the original log here would depend on which application is in
  # front). It tests the check, not the snapshot: the snapshot says NO for real in sitting 1 (the
  # native panel without the prime). Earlier versions relied on an
  # application taking the focus on its own (a background-started spike, `open -a`); that did not
  # repeat, so it was no control.
  : > "$BASE/lsof.log"
  "$BENCH/run.sh" control-ac23 > /dev/null
  wait_for 30 grep -q READY "$(run_log control-ac23)"
  sleep 2
  sendcmd control-ac23 "snapshot before"; sleep 0.5
  sendcmd control-ac23 "show"; sleep 1.5
  sendcmd control-ac23 "snapshot after"; sleep 0.5
  sendcmd control-ac23 "hide"; sleep 0.5
  sendcmd control-ac23 "quit"; sleep 2
  local shown; shown="$(last_ts "$(run_log control-ac23)" SHOWN)"
  printf '%s KEY key=KeyA\n' "$((shown + 500))" > "$BASE/control-page.log"
  sed -e 's#\( SNAP after front_pid=\)[0-9]*#\1999999#' "$(run_log control-ac23)" > "$BASE/control-altered.log"
  local line; line="$(analyze ac23 "$BASE/control-altered.log" "$BASE/control-page.log")"
  case "$line" in
    "AC-23 NO "*) res "CONTROL AC-23 PASS (the check says NO when the after snapshot has another frontmost pid): $line" ;;
    *) res "CONTROL AC-23 FAIL (the check did not say NO): $line" ;;
  esac
  collect control-ac23 control-ac23
  # AC-31 control: a synthetic wildcard listener must make the check say NO.
  printf '# sample 1\nCOMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\nm0b06 1 u 9u IPv4 0x1 0t0 TCP *:8080 (LISTEN)\n# lsof exit=0\n' > "$BASE/synthetic-lsof.log"
  line="$(analyze lsof "$BASE/synthetic-lsof.log")"
  case "$line" in
    "AC-31 NO "*) res "CONTROL AC-31 PASS (synthetic wildcard listener): $line" ;;
    *) res "CONTROL AC-31 FAIL: $line" ;;
  esac
  hashes
}

# A page click after time $1 inside the 80 point square centred on $2,$3.
click_inside() {
  awk -v t="$1" -v px="$2" -v py="$3" '$2=="CLICK" && $1>t {
    x=$3; y=$4; sub(/^x=/,"",x); sub(/^y=/,"",y)
    if (x-px <= 40 && px-x <= 40 && y-py <= 40 && py-y <= 40) f=1
  } END{exit !f}' "$PAGELOG"
}
# A click inside the square, or any click that the panel logged, after time $1.
click_or_panel_click() {
  click_inside "$1" "$2" "$3" || awk -v t="$1" '$2=="PANEL_CLICK" && $1>t{f=1} END{exit !f}' "$4"
}

# The phase of one window mechanism inside the full-screen test window.
phase() { # mechanism run-name first-target-index
  local mech="$1" run="$2" idx="$3" log; log="$(run_log "$run")"
  local left top width height shown
  state ready "Get ready ($mech). Do nothing. A pink bar will appear near the bottom of the screen in a moment."
  sleep 2
  # control of AC-24: before the show there is no bar in the shot
  sendcmd "$run" "move 1"; sleep 1
  read -r left top width height < <(grep RECT_PX "$log" | tail -1 | awk '{print $3, $4, $5, $6}')
  read -r s e file < <(shot absent "$mech")
  res "AC-24 control ($mech) before show: $(shot_check absent "$file" "$left" "$top" "$width" "$height")"
  # AC-23
  sendcmd "$run" "snapshot before"; sleep 0.5
  local precond; precond="$(grep ' SNAP before ' "$log" | tail -1)"
  local fpid; fpid="$(printf '%s' "$precond" | sed -n 's/.*front_pid=\([0-9]*\).*/\1/p')"
  if ps -axww -o pid=,command= | grep -F -- "--user-data-dir=$PROF" | awk '{print $1}' | grep -qx "$fpid" || [ -n "${M0B06_SIMULATE:-}" ]; then
    res "PRECONDITION ($mech) the test window's browser is the frontmost application: yes (front_pid=$fpid)"
  else
    res "PRECONDITION ($mech) the test window's browser is the frontmost application: NO (front_pid=$fpid): AC-23 is NOT RUN for this mechanism"
  fi
  sendcmd "$run" "show"
  wait_for 10 grep -q " SHOWN" "$log"
  shown="$(last_ts "$log" SHOWN)"
  sleep_until $((shown + 600)); sendcmd "$run" "snapshot after"
  sendcmd "$run" "windows"
  sleep_until $((shown + 950))
  read -r s1 e1 file1 < <(shot 1s "$mech")
  state key "Step 3 ($mech). Press ONE key now, within 50 seconds. Then keep your hands off the keyboard and the mouse until the next step."
  local key_seen=no
  if wait_for 50 awk -v t="$shown" '$2=="KEY" && $1>t{f=1} END{exit !f}' "$PAGELOG"; then key_seen=yes; hlog "KEY seen"; else hlog "KEY not seen in 50 s"; fi
  sleep_until $((shown + 59000))
  read -r s2 e2 file2 < <(shot 60s "$mech")
  if [ "$key_seen" = yes ]; then
    res "AC-23 ($mech): $(analyze ac23 "$log" "$PAGELOG")"
  else
    res "AC-23 ($mech): NOT RUN no key reached the test page within 50 s after SHOWN (the key was not pressed, or it did not arrive; this is not a verdict on the window)"
  fi
  res "AC-24 ($mech) 1 s timing: $(analyze shot-timing "$shown" "$s1" "$e1" 1000 500)"
  res "AC-24 ($mech) 1 s shot: $(shot_check panel "$file1" "$left" "$top" "$width" "$height")"
  res "AC-24 ($mech) 60 s timing: $(analyze shot-timing "$shown" "$s2" "$e2" 60000 2000)"
  res "AC-24 ($mech) 60 s shot: $(shot_check panel "$file2" "$left" "$top" "$width" "$height")"
  # AC-25 and its control
  local px py; read -r px py < <(grep PAGE_POINT "$log" | tail -1 | awk '{print $3, $4}')
  state click1 "Step 4 ($mech). Click once in the yellow square. It is under the pink bar: click through the bar." "$px" "$py"
  wait_for 20 has_targets "$idx"
  local t1; t1="$(awk '$2=="TARGET"{n++; if(n=='"$idx"') print $1}' "$PAGELOG")"
  wait_for 120 click_inside "$t1" "$px" "$py" || hlog "no click inside the square in 120 s"
  sleep 1
  sendcmd "$run" "clickthrough off"; sleep 0.5
  state click2 "Step 5 ($mech). Click the yellow square again. This is a control: the bar will catch this click." "$px" "$py"
  wait_for 20 has_targets $((idx + 1))
  local t2; t2="$(awk '$2=="TARGET"{n++; if(n=='"$((idx + 1))"') print $1}' "$PAGELOG")"
  wait_for 120 click_or_panel_click "$t2" "$px" "$py" "$log" || hlog "no control click in 120 s"
  sleep 1
  sendcmd "$run" "clickthrough on"; sleep 0.5
  res "AC-25 ($mech): $(analyze ac25 "$log" "$PAGELOG" "$idx" normal)"
  res "AC-25 control ($mech): $(analyze ac25 "$log" "$PAGELOG" $((idx + 1)) control)"
  # AC-29
  state cycle "Step 6 ($mech). Do nothing. The bar will show and hide 10 times (about 15 seconds)."
  sleep 1
  sendcmd "$run" "cycle 10"
  wait_for 90 bash -c "[ \"\$(grep -c ' SNAP cyc-' '$log')\" -ge 20 ]" || hlog "cycle did not finish in 90 s"
  res "AC-29 ($mech): $(analyze ac29 "$log")"
  sendcmd "$run" "hide"; sleep 0.5
}

do_sitting() {
  : > "$EVID/results.txt"; : > "$EVID/harness.log"; : > "$EVID/shots.log"
  hlog "SITTING start; macOS $(sw_vers -productVersion); displays=$(system_profiler SPDisplaysDataType | grep -c Resolution); simulate=${M0B06_SIMULATE:-0}"
  : > "$BASE/lsof.log"
  start_server
  state wait "Waiting for the harness."
  # Both spikes start first and take the focus once at launch; the browser starts last, so it is
  # the frontmost application when the panel shows.
  start_spike sit-flags
  if [ -n "${M0B06_NO_PRIME:-}" ]; then
    start_spike sit-native --mechanism native
  else
    start_spike sit-native --mechanism native --prime
    wait_for 20 grep -q PRIMED "$(run_log sit-native)" || hlog "native prime did not finish"
  fi
  if [ -n "${M0B06_SIMULATE:-}" ]; then
    python3 -I "$HERE/fake_page.py" "$PORT" > /dev/null 2>&1 &
    FAKE_PID=$!
  else
    local browser="${M0B06_BROWSER:?set M0B06_BROWSER to the path of a browser app that accepts --user-data-dir}"
    mkdir -p "$PROF"
    open -n -a "$browser" --args --user-data-dir="$PROF" --no-first-run --no-default-browser-check \
      --disable-background-networking --disable-component-update --app="http://127.0.0.1:$PORT/"
  fi
  state fullscreen "Step 1. Make this window full-screen: press Ctrl+Cmd+F (or click the green button), then click once anywhere on this page."
  wait_for 300 bash -c "'$BIN' analyze fullscreen '$PAGELOG' | grep -q 'FULLSCREEN PASS'" || { res "FULLSCREEN NOT RUN: the test window did not become full-screen in 300 s"; exit 4; }
  local fs; fs="$(last_ts "$PAGELOG" FULLSCREEN)"
  wait_for 120 awk -v t="$fs" '$2=="CLICK"{f=1} END{exit !f}' "$PAGELOG" || hlog "no focus click in 120 s"
  res "$(analyze fullscreen "$PAGELOG")"
  phase flags sit-flags 1
  phase native sit-native 3
  state done "Done. You can leave full-screen now (Ctrl+Cmd+F) and close this window."
  sendcmd sit-flags quit; sendcmd sit-native quit
  sleep 3
  for r in sit-flags sit-native; do
    res "AC-31 ($r): $(analyze lsof "$RUNS/$r/lsof.log")"
  done
  sanitize < "$PAGELOG" > "$EVID/page.log"
  collect sit-flags flags; collect sit-native native
  hashes
}

case "$MODE" in
  controls) do_controls ;;
  sitting) do_sitting ;;
  *) echo "usage: evidence.sh controls|sitting <evidence-dir>" >&2; exit 64 ;;
esac
