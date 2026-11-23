#!/usr/bin/env bash
# Run the M0b-05 checks unattended and keep the raw logs.
#
# Usage: M0B05_BROWSER=<path of a browser app that accepts --user-data-dir> \
#          bench/M0b-05/harness/evidence.sh <out-dir> [step ...]
# Steps: meta grant deny ac16r ac18r ac11 ac12 ac13 ac14 ac16 ac18 ac35 ac19 ac20 ac36g ac17 ac40 ac36r ac21 ac22 ac39
#        (default: all of them in the order meta grant ac11 .. ac36g deny ac17 ac40 ac36r grant ac21 ac22 ac39).
#        `grant` and `deny` reset the spike's permission and wait for its system dialog to be
#        answered from outside (Allow, or Don't Allow); the checks before `deny` need it granted,
#        ac17 ac40 ac36r need it refused.
#
# System effects, each printed as an `EFFECT` line before it starts, and always undone on exit:
#   - the output volume is lowered (and restored),
#   - test sounds play (a speech clip, a 1 kHz tone, a system notification sound),
#   - the sample rate of the default output device is changed (AC-18) and put back,
#   - two browser windows open with their own temporary profiles (deleted on exit).
# No key event, no input-source change and no clipboard write. No audio is written to disk:
# the spike prints level readings only.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$BENCH/../.." && pwd)"
OUT="${1:?usage: evidence.sh <out-dir> [step ...]}"
shift
STEPS=("$@")
[ "${#STEPS[@]}" -gt 0 ] || STEPS=(meta grant ac11 ac12 ac13 ac14 ac16 ac18 ac35 ac19 ac20 ac36g deny ac17 ac40 ac36r grant ac21 ac22 ac39)
BROWSER="${M0B05_BROWSER:?set M0B05_BROWSER to the path of a browser app that accepts --user-data-dir}"

APP="$BENCH/target/bundle/m0b05.app"
BIN="$APP/Contents/MacOS/m0b05"
TMPB="${TMPDIR:-/tmp}"
TMPB="${TMPB%/}"
BASE="$TMPB/m0b-05"
WORK="$TMPB/m0b05-work"
CLIPS="$TMPB/m0b05-clips"
PROF_A="$TMPB/m0b05-browser-a"
PROF_B="$TMPB/m0b05-browser-b"
MARK_A="--user-data-dir=$PROF_A"
MARK_NONE="--user-data-dir=$TMPB/m0b05-no-such-app"
VOLUME_TEST=20
# The file names and the size that the AC-21 scan treats as audio.
AUDIO_TYPES=(-name '*.wav' -o -name '*.caf' -o -name '*.aif*' -o -name '*.m4a' -o -name '*.mp3' -o -name '*.flac' -o -name '*.ogg' -o -name '*.pcm' -o -name '*.raw' -o -size +1M)
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
H="$OUT/${M0B05_HARNESS_LOG:-harness.log}"

now_ms() { python3 -c 'import time; print(int(time.time()*1000))'; }
hlog() { echo "$(now_ms) $*" | tee -a "$H"; }
effect() { hlog "EFFECT $*"; }

ORIG_VOLUME=""
ORIG_MUTED=""
ORIG_RATE=""
SERVER_PID=""
CURRENT_RUN=""

# --- cleanup: always undo every effect ----------------------------------------------------
# Kill a pid read from a file, only when it is a plain number above 1 and its command is the spike.
kill_spike_pid() { # pid
  case "$1" in '' | *[!0-9]*) return ;; esac
  [ "$1" -gt 1 ] || return
  ps -p "$1" -o command= 2>/dev/null | grep -q "/m0b05.app/" && kill -KILL "$1" 2>/dev/null
}

cleanup() {
  trap - EXIT INT TERM HUP
  if [ -n "$CURRENT_RUN" ] && [ -s "$BASE/runs/$CURRENT_RUN/pid" ]; then
    kill_spike_pid "$(cat "$BASE/runs/$CURRENT_RUN/pid")"
  fi
  pkill -f "$CLIPS/" 2>/dev/null
  if [ -n "$ORIG_RATE" ]; then
    cur="$("$BIN" rate get 2>/dev/null | sed -n 's/.*hz=\([0-9.]*\).*/\1/p')"
    if [ -n "$cur" ] && [ "${cur%.*}" != "${ORIG_RATE%.*}" ]; then
      hlog "RESTORE output sample rate to $ORIG_RATE"
      "$BIN" rate set "$ORIG_RATE" >/dev/null 2>&1 || hlog "WARN the output sample rate could not be restored: set it back to $ORIG_RATE"
    fi
  fi
  for marker in "$PROF_A" "$PROF_B"; do
    ps -axww -o pid=,command= | grep -F -- "--user-data-dir=$marker" | grep -v grep | awk '{print $1}' | xargs kill 2>/dev/null
  done
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
  if [ -n "$ORIG_VOLUME" ]; then
    hlog "RESTORE output volume to $ORIG_VOLUME"
    osascript -e "set volume output volume $ORIG_VOLUME" >/dev/null 2>&1 || hlog "WARN the output volume could not be restored: set it back to $ORIG_VOLUME"
  fi
  if [ -n "$ORIG_MUTED" ] && [ "$ORIG_MUTED" = false ]; then
    osascript -e "set volume without output muted" >/dev/null 2>&1
  fi
  hlog "RESTORE tccutil reset AudioCapture app.interlo.bench.m0b05 (the spike's own permission only)"
  tccutil reset AudioCapture app.interlo.bench.m0b05 >/dev/null 2>&1
  cp "$WORK/serve.log" "$OUT/page-server.log" 2>/dev/null
  rm -rf "$PROF_A" "$PROF_B" "$WORK" 2>/dev/null
  hlog "CLEANUP done"
}
# A signal runs the cleanup and then stops the script, so no later step plays sound at the
# restored volume.
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM
trap 'cleanup; exit 129' HUP

# --- helpers -----------------------------------------------------------------------------
step_dir() { D="$OUT/$1"; mkdir -p "$D"; }

# result <file> <AC> <STATUS> <detail>
result() { echo "$2 $3 $4" | tee -a "$1" >/dev/null; hlog "RESULT $2 $3 $4"; }

# Run the analyzer; its first word is PASS or NO. Print "PASS ..." or "NO ...".
analyze() { "$BIN" analyze "$@" 2>&1; }

set_mode() { # id mode
  printf '{"mode":"%s"}' "$2" > "$WORK/state-$1.json"
}

start_run() { # name spike-args...
  CURRENT_RUN="$1"
  bash "$BENCH/run.sh" "$@" | tee -a "$H" >/dev/null
  RUN="$BASE/runs/$CURRENT_RUN"
}

ctl() { printf '%s\n' "$1" > "$RUN/stdin.fifo"; }

wait_log() { # pattern timeout_s
  local i=0
  while ! grep -q -- "$1" "$RUN/spike.log" 2>/dev/null; do
    sleep 0.2
    i=$((i + 1))
    [ "$i" -ge $(($2 * 5)) ] && return 1
  done
  return 0
}

stop_run() { # send SIGINT, wait for the EXIT line, kill the spike if it does not exit
  local pid
  pid="$(cat "$RUN/pid" 2>/dev/null)"
  case "$pid" in '' | *[!0-9]*) pid="" ;; esac
  [ -n "$pid" ] && [ "$pid" -gt 1 ] && kill -INT "$pid" 2>/dev/null
  local i=0
  while ! grep -q "EXIT code=" "$RUN/spike.log" 2>/dev/null && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
  if [ "$i" -ge 100 ]; then
    hlog "WARN the spike did not exit within 10 s after SIGINT: killing it"
    kill_spike_pid "$pid"
  fi
  sleep 1.5 # let the sampler write its last sample and copy lsof.log
  CURRENT_RUN=""
}

# The exit code of the last run, from the spike's own EXIT line.
exit_code_of() { sed -n 's/.* EXIT code=\(-\{0,1\}[0-9]*\).*/\1/p' "$1" | tail -1; }

save_run() { # step-dir prefix
  cp "$RUN/spike.log" "$1/$2-spike.log" 2>/dev/null
  cp "$RUN/spike.err" "$1/$2-spike.err" 2>/dev/null
  cp "$RUN/lsof.log" "$1/$2-lsof.log" 2>/dev/null || cp "$BASE/lsof.log" "$1/$2-lsof.log" 2>/dev/null
}

# The result of the analyzer as a status word.
status_of() { case "$1" in PASS*) echo PASS ;; *) echo NO ;; esac; }

# A negative control must come out NO.
control() { # result-file AC label analyzer-output
  case "$4" in
    NO*) result "$1" "$2" "CONTROL-OK($3)" "$4" ;;
    *) result "$1" "$2" "CONTROL-FAILED($3)" "$4" ;;
  esac
}

# The permission code as the signed bundle sees it. It must run through `open`: a command run
# straight from a shell is judged as the terminal app, not as the bundle.
permission_code() {
  local f="$WORK/permission-code.txt"
  rm -f "$f"
  open -n -W --stdout "$f" -a "$APP" --args permission-code 2>/dev/null
  tr -d '[:space:]' < "$f" 2>/dev/null
}
need_granted() { [ "$(permission_code)" = "0" ]; }
need_refused() { [ "$(permission_code)" = "1" ]; }

# make_tone <file> <seconds of silence first> <seconds of 1 kHz tone>: a test clip, kept outside the repository
make_tone() {
  python3 - "$1" "$2" "$3" <<'PY'
import math, struct, sys, wave
path, silence, tone = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
w = wave.open(path, "wb"); w.setnchannels(1); w.setsampwidth(2); w.setframerate(48000)
w.writeframes(b"\x00\x00" * (48000 * silence))
w.writeframes(b"".join(struct.pack("<h", int(0.3 * 32767 * math.sin(2 * math.pi * 1000 * i / 48000))) for i in range(48000 * tone)))
w.close()
PY
}

# --- environment setup ---------------------------------------------------------------------
setup_env() {
  mkdir -p "$WORK" "$BASE" "$CLIPS"
  ORIG_VOLUME="$(osascript -e 'output volume of (get volume settings)' 2>/dev/null)"
  ORIG_MUTED="$(osascript -e 'output muted of (get volume settings)' 2>/dev/null)"
  case "$ORIG_VOLUME" in '' | *[!0-9]*) hlog "ABORT the output volume cannot be read ($ORIG_VOLUME), so it could not be restored"; exit 2 ;; esac
  ORIG_RATE="$("$BIN" rate get | sed -n 's/.*hz=\([0-9.]*\).*/\1/p')"
  hlog "STATE original volume=$ORIG_VOLUME output_rate=$ORIG_RATE"
  if [ ! -s "$CLIPS/speech.wav" ]; then
    hlog "MAKE test clips (speech synthesis; kept outside the repository)"
    say -v Samantha -o "$CLIPS/speech.aiff" "Hello, this is a short test of the meeting audio. The quick brown fox jumps over the lazy dog. Please confirm that you can hear me clearly. Numbers: one, two, three, four, five, six, seven, eight, nine, ten."
    afconvert -f WAVE -d LEI16@48000 -c 1 "$CLIPS/speech.aiff" "$CLIPS/speech.wav"
    make_tone "$CLIPS/tone.wav" 0 12
  fi
  if [ ! -s "$CLIPS/tone-delayed.wav" ]; then
    make_tone "$CLIPS/tone-delayed.wav" 7 14
  fi
  touch /tmp/m0b-05.start
  hlog "TOUCH /tmp/m0b-05.start (the clips were made before this line)"
  effect "lowering the output volume from $ORIG_VOLUME to $VOLUME_TEST (restored on exit)"
  osascript -e "set volume output volume $VOLUME_TEST"
  rm -f "$WORK"/state-*.json
  set_mode a off
  set_mode b off
  python3 "$HERE/serve.py" "$WORK/state" "$CLIPS" "$WORK/port" > "$WORK/serve.log" 2>&1 &
  SERVER_PID=$!
  local i=0
  while [ ! -s "$WORK/port" ] && [ "$i" -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
  PORT="$(cat "$WORK/port")"
  hlog "SERVER loopback test page on port $PORT"
  for id in a b; do
    local prof="$PROF_A"
    [ "$id" = b ] && prof="$PROF_B"
    mkdir -p "$prof"
    effect "opening browser window '$id' with its own temporary profile (deleted on exit)"
    open -n -a "$BROWSER" --args --user-data-dir="$prof" --no-first-run --no-default-browser-check \
      --disable-background-networking --disable-sync --disable-component-update \
      --autoplay-policy=no-user-gesture-required --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows --window-size=420,300 "http://127.0.0.1:$PORT/?id=$id"
  done
  # wait until each instance has a process with a Core Audio object (it is using audio)
  for prof in "$PROF_A" "$PROF_B"; do
    local i=0
    until "$BIN" target "--user-data-dir=$prof" > /dev/null 2>&1; do
      sleep 0.5
      i=$((i + 1))
      if [ "$i" -ge 60 ]; then hlog "WARN a browser instance has no audio process object after 30 s"; break; fi
    done
  done
  hlog "TARGETS a: $("$BIN" target "$MARK_A")  b: $("$BIN" target "--user-data-dir=$PROF_B")"
}

# --- steps -------------------------------------------------------------------------------
step_meta() {
  step_dir 00-meta
  {
    echo "date_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "macos=$(sw_vers -productVersion)"
    echo "mac_model=$(sysctl -n hw.model)"
    echo "commit=$(git -C "$ROOT" rev-parse HEAD)"
    echo "dirty_files=$(git -C "$ROOT" status --porcelain | wc -l | tr -d ' ')"
    echo "rustc=$(rustc --version)"
    echo "bundle_id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")"
    echo "binary_sha256=$(shasum -a 256 "$BIN" | awk '{print $1}')"
    echo "permission_code=$(permission_code) (0 granted, 1 refused, 2 not decided)"
    echo "output_rate_hz=$ORIG_RATE"
    echo "volume_before=$ORIG_VOLUME volume_during=$VOLUME_TEST"
  } > "$D/meta.txt"
  codesign --verify --deep --strict "$APP" > "$D/codesign-verify.txt" 2>&1
  echo "exit=$?" >> "$D/codesign-verify.txt"
  hlog "META written"
}

# A run that plays something for 10 s from a browser page after IDLE_TONE and marks the start.
play_period() { # label mode id
  set_mode "$3" "$2"
  ctl "mark $1"
  sleep 10
  set_mode "$3" off
}

step_ac11() {
  step_dir 01-ac11
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-11 BLOCKED "permission is not granted (code $(permission_code))"; return; fi
  start_run ac11 --target-arg "$MARK_A"
  wait_log IDLE_TONE 20 || { result "$R" AC-11 "NOT RUN" "no IDLE_TONE line"; stop_run; return; }
  play_period clip_start clip a
  ctl "" # the owner presses Enter: SOURCE_END
  sleep 6
  stop_run
  save_run "$D" run
  local out; out="$(analyze ac11 "$D/run-spike.log" clip_start)"
  local code; code="$(exit_code_of "$D/run-spike.log")"
  if [ "$(status_of "$out")" = PASS ] && [ "$code" = 0 ]; then result "$R" AC-11 PASS "exit=$code $out"; else result "$R" AC-11 NO "exit=$code $out"; fi
  # negative control: nothing plays, so "audio present" must fail
  start_run ac11-control --target-arg "$MARK_A"
  wait_log IDLE_TONE 20
  ctl "mark clip_start"; sleep 10; ctl ""; sleep 6
  stop_run; save_run "$D" control
  control "$R" AC-11 "no sound plays" "$(analyze ac11 "$D/control-spike.log" clip_start)"
}

step_ac12() {
  step_dir 02-ac12
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-12 BLOCKED "permission is not granted"; return; fi
  effect "the spike plays a 1 kHz tone on the default output for 5 s"
  start_run ac12 --target-arg "$MARK_A" --self-tone
  wait_log SELF_TONE_END 30 || { result "$R" AC-12 "NOT RUN" "no SELF_TONE_END line"; stop_run; return; }
  sleep 2; stop_run; save_run "$D" run
  local out; out="$(analyze ac12 "$D/run-spike.log")"
  result "$R" AC-12 "$(status_of "$out")" "$out"
  effect "negative control: the same tone with the spike's own process put into the tap"
  start_run ac12-control --target-arg "$MARK_A" --control-include-self
  wait_log SELF_TONE_END 30
  sleep 2; stop_run; save_run "$D" control
  control "$R" AC-12 "own process included in the tap" "$(analyze ac12 "$D/control-spike.log")"
}

# Watch which processes have an output stream running (pids only), for the audible-source proof.
probe_start() { # file seconds
  "$BIN" probe-output "$2" --target-arg "$MARK_A" > "$1" 2>&1 &
  PROBE_PID=$!
}

# Number of pids that start an output stream after the probe began (not in its first line).
new_output_pids() { # probe-file
  python3 - "$1" <<'PY'
import re, sys
lines = [l for l in open(sys.argv[1]) if "OUTPUT_PROCS" in l]
def pids(l):
    return set(re.findall(r"\d+", l.split("target=")[1])) if "target=" in l else set()
if not lines:
    print(0)
else:
    base = pids(lines[0])
    seen = set()
    for l in lines[1:]:
        seen |= pids(l) - base
    print(len(seen))
PY
}

step_ac13() {
  step_dir 03-ac13
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-13 BLOCKED "permission is not granted"; return; fi
  # (a) tone from a second browser instance (its own temporary profile; the daily profile is never used)
  effect "a 1 kHz tone plays for 10 s from the second browser instance"
  start_run ac13-b --target-arg "$MARK_A"
  wait_log IDLE_TONE 20
  probe_start "$D/b-probe.log" 14
  play_period tone_b_start tone b
  sleep 1; stop_run; save_run "$D" b; wait "$PROBE_PID" 2>/dev/null
  local out; out="$(analyze ac13 "$D/b-spike.log" tone_b_start 10)"
  result "$R" AC-13-browser "$(status_of "$out")" "$out"
  # audible-source proof: the same tone, tapped at its own process, is seen as audio present
  start_run ac13-b-audible --target-arg "--user-data-dir=$PROF_B"
  wait_log IDLE_TONE 20
  set_mode b tone; ctl "mark tone_b_start"; sleep 10; set_mode b off; sleep 1
  stop_run; save_run "$D" b-audible
  local aud; aud="$(analyze ac13 "$D/b-audible-spike.log" tone_b_start 10)"
  result "$R" AC-13-browser-audible "$(case "$aud" in NO*) echo PASS ;; *) echo NO ;; esac)" "source heard when tapped at its own process: $aud"
  # (b) tone from another app (a command-line player)
  effect "a 1 kHz tone plays for 10 s from another app"
  start_run ac13-app --target-arg "$MARK_A"
  wait_log IDLE_TONE 20
  probe_start "$D/app-probe.log" 16
  sleep 2
  ctl "mark tone_app_start"
  afplay "$CLIPS/tone.wav" &
  local APLAY=$!
  sleep 10; kill "$APLAY" 2>/dev/null; sleep 1
  stop_run; save_run "$D" app; wait "$PROBE_PID" 2>/dev/null
  out="$(analyze ac13 "$D/app-spike.log" tone_app_start 10)"
  result "$R" AC-13-app "$(status_of "$out")" "$out"
  local newp; newp="$(new_output_pids "$D/app-probe.log")"
  result "$R" AC-13-app-audible-probe "$([ "$newp" -ge 1 ] && echo PASS || echo NO)" "pids that started an output stream during the run: $newp (see app-probe.log)"
  # the same player, tapped at its own process: 7 s of silence first, then the tone
  afplay "$CLIPS/tone-delayed.wav" &
  APLAY=$!
  sleep 0.5
  start_run ac13-app-audible --target-arg "$CLIPS/tone-delayed.wav"
  wait_log IDLE_TONE 20
  sleep 1
  ctl "mark tone_app_start"
  sleep 10; kill "$APLAY" 2>/dev/null; sleep 1
  stop_run; save_run "$D" app-audible
  aud="$(analyze ac13 "$D/app-audible-spike.log" tone_app_start 10)"
  result "$R" AC-13-app-audible "$(case "$aud" in NO*) echo PASS ;; *) echo NO ;; esac)" "source heard when tapped at its own process: $aud"
  # negative control: the tone plays from the target itself
  start_run ac13-control --target-arg "$MARK_A"
  wait_log IDLE_TONE 20
  play_period tone_a_start tone a
  sleep 1; stop_run; save_run "$D" control
  control "$R" AC-13 "tone plays from the target" "$(analyze ac13 "$D/control-spike.log" tone_a_start 10)"
}

step_ac14() {
  step_dir 04-ac14
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-14 BLOCKED "permission is not granted"; return; fi
  # The sound must be audible (a process starts an output stream after the trigger); the system may
  # drop a notification, so the trigger is tried up to three times, each in its own run.
  local n out newp=0
  for n in 1 2 3; do
    effect "a system notification sound is triggered once (osascript display notification, sound Glass), attempt $n"
    start_run "ac14-$n" --target-arg "$MARK_A"
    wait_log IDLE_TONE 20
    probe_start "$D/probe-$n.log" 16
    sleep 2
    ctl "mark notify"
    osascript -e 'display notification "x" sound name "Glass"' > "$D/osascript-$n.txt" 2>&1
    echo "osascript_exit=$?" >> "$D/osascript-$n.txt"
    sleep 11; stop_run; save_run "$D" "try$n"; wait "$PROBE_PID" 2>/dev/null
    out="$(analyze ac14 "$D/try$n-spike.log" notify)"
    newp="$(new_output_pids "$D/probe-$n.log")"
    [ "$newp" -ge 1 ] && break
  done
  if [ "$newp" = 0 ]; then
    result "$R" AC-14 "NOT RUN" "after 3 attempts no process started an output stream after the trigger, so the sound was not audible: $out"
  else
    result "$R" AC-14 "$(status_of "$out")" "attempt $n: pids that started an output stream after the trigger: $newp (see probe-$n.log); $out"
  fi
  # negative control: the target plays the clip during the same 10 s
  start_run ac14-control --target-arg "$MARK_A"
  wait_log IDLE_TONE 20
  play_period notify clip a
  sleep 1; stop_run; save_run "$D" control
  control "$R" AC-14 "the target plays during the 10 s" "$(analyze ac14 "$D/control-spike.log" notify)"
}

step_ac16() {
  step_dir 05-ac16
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-16 BLOCKED "permission is not granted"; return; fi
  start_run ac16 --target-arg "$MARK_A"
  wait_log SILENT 25 || hlog "no SILENT within 25 s"
  stop_run; save_run "$D" run
  local out; out="$(analyze ac16 "$D/run-spike.log")"
  result "$R" AC-16 "$(status_of "$out")" "$out"
  # negative control: the target plays the whole time, so SILENT must not come
  set_mode a clip
  start_run ac16-control --target-arg "$MARK_A"
  sleep 16
  stop_run; save_run "$D" control
  set_mode a off
  control "$R" AC-16 "the target plays the whole run" "$(analyze ac16 "$D/control-spike.log")"
}

step_ac18() {
  step_dir 06-ac18
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-18 BLOCKED "permission is not granted"; return; fi
  start_run ac18 --target-arg "$MARK_A"
  wait_log IDLE_TONE 20
  set_mode a clip
  sleep 3
  effect "changing the default output device sample rate from $ORIG_RATE to 44100 (restored on exit)"
  "$BIN" rate set 44100 | tee -a "$D/rate-change.log"
  wait_log RESTART 10 || hlog "no RESTART line"
  sleep 5
  effect "restoring the default output device sample rate to $ORIG_RATE"
  "$BIN" rate set "${ORIG_RATE%.*}" | tee -a "$D/rate-change.log"
  sleep 4
  set_mode a off
  stop_run; save_run "$D" run
  local out; out="$(analyze ac18 "$D/run-spike.log")"
  result "$R" AC-18 "$(status_of "$out")" "$out"
  # negative control: the clip plays, nothing changes the rate
  set_mode a clip
  start_run ac18-control --target-arg "$MARK_A"
  sleep 14
  stop_run; save_run "$D" control
  set_mode a off
  control "$R" AC-18 "the rate does not change" "$(analyze ac18 "$D/control-spike.log")"
}

step_ac35() {
  step_dir 07-ac35
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-35 BLOCKED "permission is not granted"; return; fi
  start_run ac35 --target-arg "$MARK_A" --ignore-2ch
  wait_log IDLE_TONE 20
  play_period clip_start clip a
  sleep 1; stop_run; save_run "$D" run
  local out; out="$(analyze routepresent "$D/run-spike.log" route=tap clip_start)"
  result "$R" AC-35 "$(status_of "$out")" "$out"
  # negative control: ask for the 2ch route name on the tap run
  control "$R" AC-35 "the check wants route=2ch on a tap run" "$(analyze routepresent "$D/run-spike.log" route=2ch clip_start)"
}

step_ac19() {
  step_dir 08-ac19
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-19 BLOCKED "permission is not granted"; return; fi
  # The tap is not used: the target is not running, so the route falls back to the 2-channel device.
  # The source is a helper that writes a 1 kHz tone into that device (see the note: routing the
  # browser's own output needs the microphone permission of the browser, so it is not done).
  start_run ac19 --target-arg "$MARK_NONE"
  wait_log IDLE_TONE 20 || { result "$R" AC-19 BLOCKED "no capture on the 2-channel route (no IDLE_TONE line)"; stop_run; save_run "$D" run; return; }
  "$BIN" play-to-2ch 12 > "$D/play-to-2ch.log" 2>&1 &
  local PLAYER=$!
  local i=0
  until grep -q "PLAY_2CH start" "$D/play-to-2ch.log" 2>/dev/null || [ "$i" -ge 100 ]; do sleep 0.1; i=$((i + 1)); done
  ctl "mark clip_start"
  sleep 10; kill "$PLAYER" 2>/dev/null; wait "$PLAYER" 2>/dev/null
  sleep 1; stop_run; save_run "$D" run
  local out; out="$(analyze routepresent "$D/run-spike.log" route=2ch clip_start)"
  result "$R" AC-19 "$(status_of "$out")" "substitute source (tone written into the 2-channel device): $out"
  # negative control: the same log must not pass as a tap run
  control "$R" AC-19 "the check wants route=tap on a 2ch run" "$(analyze routepresent "$D/run-spike.log" route=tap clip_start)"
  # negative control: nothing plays into the device
  start_run ac19-control --target-arg "$MARK_NONE"
  wait_log IDLE_TONE 20
  ctl "mark clip_start"; sleep 11
  stop_run; save_run "$D" control
  control "$R" AC-19 "nothing plays into the 2-channel device" "$(analyze routepresent "$D/control-spike.log" route=2ch clip_start)"
}

step_ac20() {
  step_dir 09-ac20
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-20 BLOCKED "permission is not granted"; return; fi
  start_run ac20 --target-arg "$MARK_NONE"
  wait_log "route=" 5; sleep 5
  stop_run; save_run "$D" run
  local out; out="$(analyze route "$D/run-spike.log" app=missing,route=2ch)"
  result "$R" AC-20 "$(status_of "$out")" "$out"
  # negative control: the target runs, so app=missing must not appear
  start_run ac20-control --target-arg "$MARK_A"
  wait_log "route=" 5; sleep 5
  stop_run; save_run "$D" control
  control "$R" AC-20 "the target app is running" "$(analyze route "$D/control-spike.log" app=missing,route=2ch)"
}

step_ac36g() {
  step_dir 10-ac36-granted
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-36-app-missing BLOCKED "permission is not granted"; return; fi
  start_run ac36g --target-arg "$MARK_NONE" --ignore-2ch
  sleep 4; stop_run; save_run "$D" run
  local out; out="$(analyze noroute "$D/run-spike.log" app=missing)"
  result "$R" AC-36-app-missing "$(status_of "$out")" "$out (permission granted, target not running, 2ch ignored)"
  control "$R" AC-36 "the check wants a different cause list" "$(analyze noroute "$D/run-spike.log" PERMISSION_REFUSED,app=missing)"
}

step_ac17() {
  step_dir 11-ac17
  local R="$D/result.txt"
  : > "$R"
  if ! need_refused; then result "$R" AC-17 BLOCKED "the permission is not in the refused state (code $(permission_code))"; return; fi
  start_run ac17 --target-arg "$MARK_A"
  wait_log "route=" 5; sleep 5
  stop_run; save_run "$D" run
  local out; out="$(analyze route "$D/run-spike.log" PERMISSION_REFUSED,route=2ch)"
  result "$R" AC-17 "$(status_of "$out")" "$out"
  control "$R" AC-17 "the check wants the no-cause route line" "$(analyze route "$D/run-spike.log" route=tap)"
}

step_ac40() {
  step_dir 12-ac40
  local R="$D/result.txt"
  : > "$R"
  if ! need_refused; then result "$R" AC-40 BLOCKED "the permission is not in the refused state"; return; fi
  start_run ac40 --target-arg "$MARK_NONE"
  wait_log "route=" 5; sleep 5
  stop_run; save_run "$D" run
  local out; out="$(analyze route "$D/run-spike.log" PERMISSION_REFUSED,app=missing,route=2ch)"
  result "$R" AC-40 "$(status_of "$out")" "$out"
  control "$R" AC-40 "the check wants one cause only" "$(analyze route "$D/run-spike.log" PERMISSION_REFUSED,route=2ch)"
}

step_ac36r() {
  step_dir 13-ac36-refused
  local R="$D/result.txt"
  : > "$R"
  if ! need_refused; then result "$R" AC-36 BLOCKED "the permission is not in the refused state"; return; fi
  start_run ac36r-both --target-arg "$MARK_NONE" --ignore-2ch
  sleep 4; stop_run; save_run "$D" both
  local out; out="$(analyze noroute "$D/both-spike.log" PERMISSION_REFUSED,app=missing)"
  result "$R" AC-36-both-causes "$(status_of "$out")" "$out"
  start_run ac36r-perm --target-arg "$MARK_A" --ignore-2ch
  sleep 4; stop_run; save_run "$D" perm
  out="$(analyze noroute "$D/perm-spike.log" PERMISSION_REFUSED)"
  result "$R" AC-36-permission-only "$(status_of "$out")" "$out"
  control "$R" AC-36 "the check wants app=missing too" "$(analyze noroute "$D/perm-spike.log" PERMISSION_REFUSED,app=missing)"
}

step_ac21() {
  step_dir 14-ac21
  local R="$D/result.txt"
  : > "$R"
  [ -d "$BASE" ] && [ -d "$HOME/Library/Caches/app.interlo.bench.m0b05" ] || { result "$R" AC-21 BLOCKED "a scanned folder does not exist"; return; }
  ( cd "$ROOT" && find bench/M0b-05 -path '*/target' -prune -o -newer /tmp/m0b-05.start -type f \( "${AUDIO_TYPES[@]}" \) -print ) > "$D/find-bench.txt" 2> "$D/find-bench.err"
  local c1=$?
  find "$BASE" "$HOME/Library/Caches/app.interlo.bench.m0b05" -newer /tmp/m0b-05.start -type f \( "${AUDIO_TYPES[@]}" \) -print > "$D/find-runtime.txt" 2> "$D/find-runtime.err"
  local c2=$?
  hlog "AC-21 find exit codes: bench=$c1 runtime=$c2; bytes printed: bench=$(wc -c < "$D/find-bench.txt" | tr -d ' ') runtime=$(wc -c < "$D/find-runtime.txt" | tr -d ' ')"
  if [ "$c1" = 0 ] && [ "$c2" = 0 ] && [ ! -s "$D/find-bench.txt" ] && [ ! -s "$D/find-runtime.txt" ] && [ ! -s "$D/find-bench.err" ] && [ ! -s "$D/find-runtime.err" ]; then
    result "$R" AC-21 PASS "both find commands exit 0 and print nothing"
  else
    result "$R" AC-21 NO "bench exit=$c1 runtime exit=$c2; see find-*.txt and find-*.err"
  fi
  # negative control: a decoy audio file newer than the start line must be found
  : > "$BASE/decoy.wav"
  find "$BASE" "$HOME/Library/Caches/app.interlo.bench.m0b05" -newer /tmp/m0b-05.start -type f \( -name '*.wav' -o -size +1M \) -print > "$D/control-find.txt" 2>&1
  rm -f "$BASE/decoy.wav"
  if [ -s "$D/control-find.txt" ]; then result "$R" AC-21 "CONTROL-OK(a decoy .wav file is found)" "$(cat "$D/control-find.txt")"; else result "$R" AC-21 "CONTROL-FAILED(a decoy .wav file is found)" "nothing printed"; fi
}

step_ac22() {
  step_dir 15-ac22
  local R="$D/result.txt"
  : > "$R"
  ( cd "$ROOT" && scripts/check.sh quick ) > "$D/check-quick.log" 2>&1
  local c=$?
  local last; last="$(grep -v '^$' "$D/check-quick.log" | tail -1)"
  ( cd "$ROOT" && cargo metadata --no-deps --format-version 1 ) > "$D/cargo-metadata-root.json" 2> "$D/cargo-metadata-root.err"
  local m; m="$(python3 - "$D/cargo-metadata-root.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(sum(1 for p in d["packages"] if "/bench/" in p["manifest_path"]))
PY
)"
  # The home directory of the person who ran this is replaced, so no user name is committed.
  sed -i '' "s#$HOME#\$HOME#g" "$D/check-quick.log" "$D/cargo-metadata-root.json"
  echo "root packages=$(python3 -c "import json,sys; print(len(json.load(open(sys.argv[1]))['packages']))" "$D/cargo-metadata-root.json") packages_under_bench=$m" > "$D/metadata-summary.txt"
  if [ "$c" = 0 ] && [ "$last" = "ALL STEPS PASSED (quick)" ] && [ "$m" = 0 ]; then
    result "$R" AC-22-gate PASS "check.sh quick exit 0, last line '$last'; packages under bench/ in the root metadata: $m"
  else
    result "$R" AC-22-gate NO "check.sh quick exit $c, last line '$last'; packages under bench/: $m"
  fi
  # negative control: the bench manifest, read directly, does list a package under bench/
  ( cd "$ROOT" && cargo metadata --no-deps --format-version 1 --manifest-path bench/M0b-05/Cargo.toml ) > "$D/control-metadata-bench.json" 2>/dev/null
  sed -i '' "s#$HOME#\$HOME#g" "$D/control-metadata-bench.json"
  local cm; cm="$(python3 - "$D/control-metadata-bench.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(sum(1 for p in d["packages"] if "/bench/" in p["manifest_path"]))
PY
)"
  if [ "$cm" -ge 1 ]; then result "$R" AC-22 "CONTROL-OK(the bench manifest read directly lists a bench package)" "packages under bench/: $cm"; else result "$R" AC-22 "CONTROL-FAILED(the bench manifest read directly lists a bench package)" "$cm"; fi
}

step_ac39() {
  step_dir 16-ac39
  local R="$D/result.txt"
  : > "$R"
  local n=0 bad=0
  for f in "$OUT"/*/*-lsof.log; do
    [ -e "$f" ] || continue
    n=$((n + 1))
    local out; out="$("$BIN" analyze lsof "$f")"
    echo "$(basename "$(dirname "$f")")/$(basename "$f"): $out" >> "$D/per-run.txt"
    case "$out" in PASS*) ;; *) bad=$((bad + 1)) ;; esac
  done
  if [ "$n" -gt 0 ] && [ "$bad" = 0 ]; then result "$R" AC-39 PASS "$n run logs, each with sample lines and no non-loopback address or wildcard listener (see per-run.txt)"; else result "$R" AC-39 NO "$n run logs, $bad failing (see per-run.txt)"; fi
  # negative control: a sample in the same format that holds a listener on every interface
  # (synthetic: no real socket is opened for it)
  printf '# sample %s\nm0b05 1 user 4u IPv4 0x1 0t0 TCP *:8080 (LISTEN)\n# lsof exit=0\n' "$(now_ms)" > "$D/control-lsof.log"
  control "$R" AC-39 "a wildcard listener is sampled" "$("$BIN" analyze lsof "$D/control-lsof.log")"
}

# Put the spike's audio-capture permission into a known state through the system dialog:
# reset it (the spike's own entry only), start the spike on the target, and wait for the
# dialog to be answered from outside (a mouse click: Allow for `grant`, Don't Allow for `deny`).
PROMPT_N=0
prompt_step() { # grant|deny
  PROMPT_N=$((PROMPT_N + 1))
  step_dir "9$PROMPT_N-permission-$1"
  local R="$D/result.txt"
  : > "$R"
  effect "tccutil reset AudioCapture app.interlo.bench.m0b05 (the spike's own permission only)"
  tccutil reset AudioCapture app.interlo.bench.m0b05 > "$D/tccutil.txt" 2>&1
  hlog "PERMISSION code before the prompt: $(permission_code)"
  start_run "prompt-$1-$PROMPT_N" --target-arg "$MARK_A"
  wait_log "route=" 90 || hlog "no route line within 90 s"
  sleep 1; stop_run; save_run "$D" run
  local code; code="$(permission_code)"
  local want=0
  [ "$1" = deny ] && want=1
  if [ "$code" = "$want" ]; then
    result "$R" "PERMISSION-$1" DONE "permission code after the prompt: $code (0 granted, 1 refused); route lines: $(grep -c 'route=' "$D/run-spike.log")"
  else
    result "$R" "PERMISSION-$1" BLOCKED "permission code after the prompt: $code, wanted $want: the dialog was not answered as needed"
  fi
}
step_grant() { prompt_step grant; }
step_deny() { prompt_step deny; }

# median, min and max of the numbers given as arguments
spread() {
  python3 - "$@" <<'PY'
import statistics, sys
v = sorted(int(x) for x in sys.argv[1:])
print(f"n={len(v)} median={statistics.median(v)} min={v[0]} max={v[-1]}")
PY
}

REPS=5

# AC-16 timing, repeated: the time from TAP_START to SILENT varies a little from run to run.
step_ac16r() {
  step_dir 17-ac16-repeat
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-16-repeat BLOCKED "permission is not granted"; return; fi
  local n out ms list=() bad=0
  for n in $(seq 1 "$REPS"); do
    start_run "ac16r-$n" --target-arg "$MARK_A"
    wait_log SILENT 25 || hlog "no SILENT within 25 s"
    stop_run; save_run "$D" "run$n"
    out="$(analyze ac16 "$D/run$n-spike.log")"
    case "$out" in PASS*) ms="$(echo "$out" | sed -n 's/.*SILENT \([0-9]*\) ms.*/\1/p')"; list+=("$ms") ;; *) bad=$((bad + 1)) ;; esac
  done
  if [ "$bad" = 0 ]; then
    result "$R" AC-16-repeat PASS "SILENT after TAP_START in ms over $REPS runs: $(spread "${list[@]}") (limit 11000)"
  else
    result "$R" AC-16-repeat NO "$bad of $REPS runs failed"
  fi
}

# AC-18 timing, repeated: restart delay and time to the first audio-present window.
step_ac18r() {
  step_dir 18-ac18-repeat
  local R="$D/result.txt"
  : > "$R"
  if ! need_granted; then result "$R" AC-18-repeat BLOCKED "permission is not granted"; return; fi
  local n out restarts=() presents=() bad=0
  for n in $(seq 1 "$REPS"); do
    start_run "ac18r-$n" --target-arg "$MARK_A"
    wait_log IDLE_TONE 20
    set_mode a clip
    sleep 3
    effect "changing the default output device sample rate from $ORIG_RATE to 44100 (restored right after), repeat $n"
    "$BIN" rate set 44100 >> "$D/rate-change.log"
    wait_log RESTART 10 || hlog "no RESTART line"
    sleep 4
    "$BIN" rate set "${ORIG_RATE%.*}" >> "$D/rate-change.log"
    sleep 3
    set_mode a off
    stop_run; save_run "$D" "run$n"
    out="$(analyze ac18 "$D/run$n-spike.log")"
    case "$out" in
      PASS*) restarts+=("$(echo "$out" | sed -n 's/.*restart_after_rate_change_ms=\([0-9]*\).*/\1/p')"); presents+=("$(echo "$out" | sed -n 's/.*after_restart=\([0-9]*\).*/\1/p')") ;;
      *) bad=$((bad + 1)) ;;
    esac
  done
  if [ "$bad" = 0 ]; then
    result "$R" AC-18-repeat PASS "over $REPS runs: RESTART after RATE_CHANGE in ms: $(spread "${restarts[@]}") (limit 3000); first audio-present one-second window ends after RESTART in ms: $(spread "${presents[@]}") (limit 3000)"
  else
    result "$R" AC-18-repeat NO "$bad of $REPS runs failed"
  fi
}

# --- main --------------------------------------------------------------------------------
[ -x "$BIN" ] || { echo "evidence.sh: build the bundle first: bench/M0b-05/bundle.sh" >&2; exit 2; }
setup_env
for s in "${STEPS[@]}"; do
  hlog "STEP $s start"
  "step_$s"
  hlog "STEP $s end"
done
