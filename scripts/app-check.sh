#!/usr/bin/env bash
# Checks that need the real signed app: the build flags, the window, the sockets
# and the memory. It is run by `scripts/check.sh app` and is not part of the
# `full` gate: it needs a display, the Screen Recording permission of the calling
# app and the local signing identity.
#
#   scripts/app-check.sh prereq          tools, identity, display and Screen Recording; starts the evidence folder
#   scripts/app-check.sh build-proof     rebuild the crate and keep its rustc command lines
#   scripts/app-check.sh bundle          build the .app, sign it, verify the signature
#   scripts/app-check.sh window          title, size and first frame within 60 s
#   scripts/app-check.sh idle            three launches: sockets for 60 s, memory samples
#   scripts/app-check.sh controls        negative controls: each check must be able to fail
#   scripts/app-check.sh evidence        SUMMARY.md and SHA256SUMS
#   scripts/app-check.sh override-guard  refuses an environment that overrides the release profile
#
# Evidence: every step writes its raw output into $APP_CHECK_EVIDENCE (default
# docs/m0/M0b-01/evidence). Nothing is summarised away: the raw files carry the
# timestamps, pids and command lines, SUMMARY.md only quotes them and SHA256SUMS
# covers every file. Text logs and PNGs of the Interlo window only.
#
# Exit codes: 0 pass, 1 fail, 3 BLOCKED (a tool, a permission or an identity is
# missing; the environment does not provide it). A blocked step is never a pass.
# Written for bash 3.2 (the macOS default): no associative arrays, no mapfile.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${APP_CHECK_DIR:-$ROOT/target/app-check}"
EVID="${APP_CHECK_EVIDENCE:-$ROOT/docs/m0/M0b-01/evidence}"
APP="$ROOT/target/release/bundle/macos/Interlo.app"
BIN="$APP/Contents/MacOS/interlo-desktop"
IDENTITY="${APP_CHECK_IDENTITY:-Interlo Local Signing}"
HELPER_SRC="$ROOT/scripts/app-check/window-info.swift"
HELPER="$OUT/window-info"
TAURI_MANIFEST="apps/desktop/src-tauri/Cargo.toml"
SELF="$ROOT/scripts/app-check.sh"

# The limits are the starting guesses of the SPEC (AC-1, AC-8). Memory gets no
# pass line (SPEC section 5): the readings must exist and their spread is printed.
WINDOW_TITLE="Interlo"
WINDOW_WIDTH=1200
WINDOW_HEIGHT=780
WINDOW_DEADLINE_MS=60000
SHOT_LATE_AFTER_S=5
IDLE_SECONDS=60
IDLE_LAUNCHES=3
RSS_SAMPLES=5

trap 'echo "internal error: line $LINENO exited with $?" >&2' ERR

blocked() {
  echo "BLOCKED: $1"
  exit 3
}

fail() {
  echo "FAIL: $1"
  exit 1
}

now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'; }
iso() { date '+%Y-%m-%dT%H:%M:%S%z'; }
# Sleep until the given epoch millisecond (returns at once when it has passed).
sleep_until_ms() { perl -MTime::HiRes=sleep,time -e 'my $d = $ARGV[0] / 1000 - time; sleep($d) if $d > 0' "$1"; }
# Keeps the home directory out of the evidence files.
redact() { sed "s#$HOME#~#g"; }

# ---------------------------------------------------------------- pure helpers

# Reads `lsof -F pPn` output on stdin and prints one line per socket that breaks
# the rule: a wildcard address, or a local or remote address that is not loopback.
# Prints nothing when every socket is loopback only.
socket_violations() {
  awk '
    function host(addr,   h) {
      h = addr
      sub(/:[^:]*$/, "", h)           # drop the port
      gsub(/^\[|\]$/, "", h)          # IPv6 brackets
      return h
    }
    function loopback(h) {
      return (h ~ /^127\./ || h == "::1" || h == "localhost")
    }
    /^p/ { pid = substr($0, 2); next }
    /^P/ { proto = substr($0, 2); next }
    /^n/ {
      name = substr($0, 2)
      sub(/ \(.*\)$/, "", name)       # drop the state, for example (LISTEN)
      n = split(name, parts, "->")
      bad = 0
      for (i = 1; i <= n; i++) {
        h = host(parts[i])
        if (h == "*" || h == "0.0.0.0" || h == "::" || !loopback(h)) bad = 1
      }
      if (bad) print "pid " pid " " proto " " name
    }
  '
}

# Median of the numbers on stdin (one per line); the lower middle for an even count.
median() {
  sort -n | awk '{ v[NR] = $1 } END { if (NR == 0) exit 1; print v[int((NR + 1) / 2)] }'
}

min_of() { sort -n | head -1; }
max_of() { sort -n | tail -1; }

# One rustc command line against the agreed profile. Prints the reason and returns
# 1 when it differs. kind is the crate type of the line (lib or bin).
assert_rustc_line() { # line kind
  local line="$1" kind="$2"
  echo "$line" | grep -qE -- '-C opt-level=3( |$)' || { echo "$kind target: no -C opt-level=3"; return 1; }
  echo "$line" | grep -qE -- '-C codegen-units=1( |$)' || { echo "$kind target: no -C codegen-units=1"; return 1; }
  echo "$line" | grep -qE -- '-C strip=(symbols|debuginfo)( |$)' || { echo "$kind target: no -C strip flag"; return 1; }
  if echo "$line" | grep -qE -- '-C panic='; then
    echo "$kind target: a -C panic flag is set (unwinding must stay the default)"
    return 1
  fi
  if [ "$kind" = "bin" ]; then
    echo "$line" | grep -qE -- '-C lto=thin( |$)' || { echo "bin target: no -C lto=thin"; return 1; }
  fi
  return 0
}

# The window reading against the limits. Returns 0 pass, 1 fail, 3 blocked.
assert_window() { # title width height elapsed_ms
  local title="$1" w="$2" h="$3" elapsed="$4"
  if [ -z "$title" ]; then
    echo "the window title is empty: Screen Recording is not granted to the app that runs this script"
    return 3
  fi
  [ "$title" = "$WINDOW_TITLE" ] || { echo "window title is \"$title\", expected \"$WINDOW_TITLE\""; return 1; }
  if [ "$w" != "$WINDOW_WIDTH" ] || [ "$h" != "$WINDOW_HEIGHT" ]; then
    echo "window size is ${w} by ${h}, expected ${WINDOW_WIDTH} by ${WINDOW_HEIGHT}"
    return 1
  fi
  [ "$elapsed" -le "$WINDOW_DEADLINE_MS" ] || { echo "the window took ${elapsed} ms (limit ${WINDOW_DEADLINE_MS} ms)"; return 1; }
  return 0
}

# A memory file must hold at least RSS_SAMPLES readings (lines with rss_kb=<number>).
assert_rss_file() { # file
  local n
  n="$(grep -cE 'rss_kb=[0-9]+$' "$1" || true)"
  [ "$n" -ge "$RSS_SAMPLES" ] || { echo "$1: $n readings, expected at least $RSS_SAMPLES"; return 1; }
  return 0
}

rss_values() { sed -n 's/.*rss_kb=\([0-9][0-9]*\)$/\1/p' "$1"; }

# ------------------------------------------------------------------ process helpers

# Pid of the process whose command is exactly the app binary.
app_pid() {
  ps -axo pid=,command= | awk -v b="$BIN" '{ pid = $1; $1 = ""; sub(/^ /, ""); if ($0 == b) { print pid; exit } }'
}

stop_app() {
  pkill -f "^$BIN\$" 2>/dev/null || true
  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do
    [ -z "$(app_pid)" ] && return 0
    sleep 0.5
  done
  pkill -9 -f "^$BIN\$" 2>/dev/null || true
  sleep 1
}

# Pids of every WebKit helper process on the machine, one per line.
webkit_pids() { ps -axo pid=,command= | awk '/com\.apple\.WebKit\.(WebContent|Networking|GPU)/ { print $1 }' | sort; }

# The pid and every descendant of it.
tree_pids() {
  local pid="$1" child
  echo "$pid"
  for child in $(pgrep -P "$pid" 2>/dev/null || true); do
    tree_pids "$child"
  done
}

# The WebKit helpers that did not exist before the launch. They have the parent
# process 1 (they are started on behalf of the app), so the parent cannot say
# which app they serve: a helper that another app starts during the run would be
# counted too, which can only make the socket check stricter.
helper_pids() { comm -13 "$OUT/webkit.before" <(webkit_pids); }

all_pids() { { tree_pids "$1"; helper_pids; } | sort -un | tr '\n' ',' | sed 's/,$//'; }

launch_app() {
  mkdir -p "$OUT"
  stop_app
  webkit_pids >"$OUT/webkit.before"
  now_ms >"$OUT/launch.t0"
  open -n "$APP"
  local i pid=""
  for i in $(seq 1 100); do
    pid="$(app_pid)"
    [ -n "$pid" ] && break
    sleep 0.1
  done
  [ -n "$pid" ] || fail "the app did not start within 10 s"
  echo "$pid"
}

# Raw `lsof -F pPn` output for a comma list of pids; empty when there is no socket.
lsof_raw() { lsof -nP -a -p "$1" -i -F pPn 2>/dev/null || true; }

# ------------------------------------------------------------------------ steps

step_override_guard() {
  local overrides="" dir f
  overrides="$(env | grep -E '^(CARGO_PROFILE_RELEASE_|CARGO_ENCODED_RUSTFLAGS=|RUSTFLAGS=)' || true)"
  [ -z "$overrides" ] || fail "an environment variable overrides the release profile: ${overrides%%=*}"
  dir="$ROOT"
  while [ "$dir" != "/" ]; do
    for f in "$dir/.cargo/config" "$dir/.cargo/config.toml"; do
      if [ -f "$f" ] && grep -qE '^[[:space:]]*(\[profile|rustflags|\[build\])' "$f"; then
        fail "$f sets a profile or rustflags"
      fi
    done
    dir="$(dirname "$dir")"
  done
  for f in "${CARGO_HOME:-$HOME/.cargo}/config" "${CARGO_HOME:-$HOME/.cargo}/config.toml"; do
    if [ -f "$f" ] && grep -qE '^[[:space:]]*(\[profile|rustflags|\[build\])' "$f"; then
      fail "$f sets a profile or rustflags"
    fi
  done
  echo "no profile or rustflags override in the environment, in .cargo/config* above the repository or in CARGO_HOME"
}

step_prereq() {
  [ "$(uname -s)" = "Darwin" ] || blocked "this check runs on macOS only"
  local tool
  for tool in cargo codesign security screencapture lsof ps pgrep swiftc perl sips awk nm shasum python3 git; do
    command -v "$tool" >/dev/null 2>&1 || blocked "$tool is not installed"
  done
  cargo tauri --version >/dev/null 2>&1 || blocked "the Tauri CLI is not installed (cargo install tauri-cli --version '^2' --locked)"
  security find-identity -v -p codesigning | grep -q "$IDENTITY" ||
    blocked "the signing identity \"$IDENTITY\" is not in the keychain"
  mkdir -p "$OUT" "$EVID/shots"
  # A new run replaces the evidence of the last run.
  find "$EVID" -type f -delete
  mkdir -p "$EVID/shots"
  swiftc -O -o "$HELPER" "$HELPER_SRC" || blocked "the window helper did not compile (is the Xcode command line tools install complete?)"
  # A denied Screen Recording permission makes screencapture fail and hides window titles.
  screencapture -x -R 0,0,16,16 "$OUT/probe.png" 2>/dev/null ||
    blocked "screencapture failed: grant Screen Recording to the app that runs this script (System Settings, Privacy and Security)"
  [ -s "$OUT/probe.png" ] || blocked "screencapture wrote no image"
  {
    echo "# Evidence of scripts/check.sh app"
    echo "date: $(iso)"
    echo "commit: $(git -C "$ROOT" rev-parse HEAD)"
    echo "uncommitted paths outside the evidence folder:"
    git -C "$ROOT" status --porcelain | grep -v 'docs/m0/M0b-01/evidence' | sed 's/^/  /' || true
    echo "macOS: $(sw_vers -productVersion) (build $(sw_vers -buildVersion))"
    echo "Mac model: $(sysctl -n hw.model), $(sysctl -n machdep.cpu.brand_string)"
    echo "cargo: $(cargo --version)"
    echo "rustc: $(rustc --version)"
    echo "tauri-cli: $(cargo tauri --version)"
    echo "swiftc: $(swiftc --version 2>&1 | head -1)"
    echo "signing identity used: $IDENTITY"
    echo "command: scripts/check.sh app"
  } >"$EVID/meta.txt"
  echo "tools, signing identity, window helper and Screen Recording: available"
  echo "evidence folder started: ${EVID#"$ROOT"/}/meta.txt"
}

step_build_proof() {
  step_override_guard
  local log="$OUT/build-verbose.log" lines line kind n=0 out="$EVID/01-build-flags.txt"
  cargo clean -p interlo-desktop --release
  cargo build --release -p interlo-desktop -v >"$log" 2>&1 || {
    tail -20 "$log"
    fail "cargo build --release -p interlo-desktop failed"
  }
  lines="$(grep 'Running `.*rustc .*--crate-name interlo_desktop ' "$log" | sed 's/.*Running `//; s/`$//')"
  [ -n "$lines" ] || fail "no rustc command line for interlo_desktop in the verbose log (the crate was not rebuilt)"
  {
    echo "# The rustc command lines that cargo ran for the crate interlo_desktop"
    echo "# $(iso), after: cargo clean -p interlo-desktop --release; cargo build --release -p interlo-desktop -v"
    echo "# The home directory is shortened to ~; nothing else is changed."
    echo
    echo "$lines" | redact | sed 's/^/COMMAND: /'
    echo
  } >"$out"
  local summary=""
  while IFS= read -r line; do
    n=$((n + 1))
    kind="$(echo "$line" | grep -oE -- '--crate-type [a-z]+' | cut -d' ' -f2)"
    local flags
    flags="$(echo "$line" | grep -oE -- '-C (opt-level|codegen-units|lto|linker-plugin-lto|strip|panic)(=[a-z0-9]+)?' | tr '\n' ' ')"
    echo "FLAGS $kind: $flags" >>"$out"
    assert_rustc_line "$line" "$kind" >>"$out" || {
      cat "$out" | tail -3
      fail "$kind target: the rustc flags do not match the profile"
    }
    summary="$summary$kind "
  done <<EOF
$lines
EOF
  [ "$n" -ge 2 ] || fail "expected the lib and the bin rustc commands, found $n"
  local symbols
  symbols="$(nm -a "$ROOT/target/release/interlo-desktop" 2>/dev/null | wc -l | tr -d ' ')"
  {
    echo "binary size (stat -f %z target/release/interlo-desktop): $(stat -f %z "$ROOT/target/release/interlo-desktop") bytes"
    echo "binary symbol table entries (nm -a target/release/interlo-desktop): $symbols"
    echo "RESULT: $n rustc commands (${summary% }) have opt-level=3, codegen-units=1, a strip flag, no panic flag; the bin has lto=thin; no profile or rustflags override found"
  } >>"$out"
  grep -E '^(FLAGS|binary|RESULT)' "$out"
}

step_bundle() {
  [ -f "$ROOT/apps/ui/dist/index.html" ] || fail "apps/ui/dist is not built (run: npm --prefix apps/ui run build)"
  stop_app
  local out="$EVID/00-bundle.txt" status
  # The Tauri CLI rewrites one line of the shell manifest; keep the file as it was.
  cp "$ROOT/$TAURI_MANIFEST" "$OUT/Cargo.toml.before"
  restore_manifest() { cp "$OUT/Cargo.toml.before" "$ROOT/$TAURI_MANIFEST"; }
  trap restore_manifest EXIT
  cargo tauri build --bundles app --config '{"bundle":{"active":true}}' >"$OUT/bundle.log" 2>&1 || {
    tail -20 "$OUT/bundle.log"
    fail "cargo tauri build failed"
  }
  restore_manifest
  trap - EXIT
  codesign --force --deep --sign "$IDENTITY" "$APP" || blocked "codesign did not sign the bundle (a keychain prompt may need Allow)"
  status=0
  codesign --verify --deep --strict --verbose=2 "$APP" >"$OUT/verify.txt" 2>&1 || status=$?
  {
    echo "# The bundle, its signature and its size ($(iso))"
    echo "bundle: target/release/bundle/macos/Interlo.app (one-off --config override, tauri.conf.json unchanged)"
    echo "\$ codesign --verify --deep --strict --verbose=2 Interlo.app"
    redact <"$OUT/verify.txt"
    echo "exit: $status"
    echo "\$ codesign -dv Interlo.app"
    codesign -dv "$APP" 2>&1 | redact | grep -E 'Identifier|Signed Time|Signature|Format' || true
    echo "\$ shasum -a 256 .../Contents/MacOS/interlo-desktop"
    shasum -a 256 "$BIN" | sed "s#$ROOT/##" | redact
    echo "\$ du -sh Interlo.app"
    du -sh "$APP" | sed "s#$ROOT/##"
    if [ "$status" -eq 0 ]; then echo "RESULT: signed with \"$IDENTITY\"; codesign --verify --deep --strict exit 0"; fi
  } >"$out"
  [ "$status" -eq 0 ] || fail "codesign --verify --deep --strict failed (exit $status)"
  grep -E '^(exit|RESULT)|^[0-9a-f]{64}|^[0-9.]+M' "$out"
}

step_window() {
  local log="$EVID/02-window-poll.log" pid start now elapsed info="" first="" id title w h epoch
  pid="$(launch_app)"
  start="$(cat "$OUT/launch.t0")"
  {
    echo "# Window poll, every 100 ms, from the moment before open -n ($(iso))"
    echo "launch: epoch_ms=$start pid=$pid; limit ${WINDOW_DEADLINE_MS} ms"
  } >"$log"
  while :; do
    now="$(now_ms)"
    elapsed=$((now - start))
    info="$("$HELPER" "$pid" || true)"
    first="$(echo "$info" | head -1)"
    echo "t=${elapsed}ms windows=$(echo "$info" | grep -c . || true) ${first:+found: $(echo "$first" | awk -F'\t' '{ printf "id=%s title=\"%s\" size=%sx%s", $1, $2, $3, $4 }')}" >>"$log"
    [ -z "$first" ] || break
    [ "$elapsed" -lt "$WINDOW_DEADLINE_MS" ] || {
      stop_app
      fail "no window within $((WINDOW_DEADLINE_MS / 1000)) s"
    }
    sleep 0.1
  done
  id="$(echo "$first" | cut -f1)"
  title="$(echo "$first" | cut -f2)"
  w="$(echo "$first" | cut -f3)"
  h="$(echo "$first" | cut -f4)"
  epoch="$(date +%s)"
  screencapture -x -o -l "$id" "$EVID/shots/first-frame-win${id}-t${elapsed}ms-epoch${epoch}.png" || fail "screencapture of window $id failed"
  echo "capture: first-frame-win${id}-t${elapsed}ms-epoch${epoch}.png taken at t=${elapsed}ms (the first poll that found the window)" >>"$log"
  sleep "$SHOT_LATE_AFTER_S"
  now="$(now_ms)"
  epoch="$(date +%s)"
  local late=$((now - start))
  screencapture -x -o -l "$id" "$EVID/shots/after-${SHOT_LATE_AFTER_S}s-win${id}-t${late}ms-epoch${epoch}.png" || fail "second screencapture failed"
  echo "capture: after-${SHOT_LATE_AFTER_S}s-win${id}-t${late}ms-epoch${epoch}.png taken at t=${late}ms" >>"$log"
  stop_app
  local shot
  for shot in "$EVID"/shots/*.png; do
    [ -s "$shot" ] || fail "$shot is empty"
    echo "$(basename "$shot"): $(sips -g pixelWidth -g pixelHeight "$shot" | awk '/pixel/ { printf "%s ", $2 }')pixels, $(wc -c <"$shot" | tr -d ' ') bytes" >>"$log"
  done
  local rc=0 msg
  msg="$(assert_window "$title" "$w" "$h" "$elapsed")" || rc=$?
  if [ "$rc" -eq 3 ]; then blocked "$msg"; fi
  [ "$rc" -eq 0 ] || fail "$msg"
  echo "RESULT: title \"$title\", size ${w} by ${h} points, found ${elapsed} ms after launch (limit ${WINDOW_DEADLINE_MS} ms); two screenshots of window $id" >>"$log"
  grep -E '^(capture|RESULT)|pixels' "$log"
  echo "screenshots are in ${EVID#"$ROOT"/}/shots/ (the owner views them before they are committed)"
}

# One launch: sample the sockets every second for IDLE_SECONDS, then sample the memory.
idle_launch() {
  local n="$1" pid i pids prev_pids="" raw lines t0 t tick total_sockets=0 samples=0
  local slog="$EVID/03-sockets-launch$n.log" rlog="$EVID/04-rss-launch$n.txt" values late=0
  pid="$(launch_app)"
  t0="$(now_ms)"
  {
    echo "# Launch $n: socket samples every 1 s for ${IDLE_SECONDS} s ($(iso))"
    echo "# Each sample: the pid list (the app, its children, the WebKit helpers started after the launch) and the raw"
    echo "# output of: lsof -nP -a -p <pids> -i -F pPn   (an empty block means no socket). The process table is written"
    echo "# when the pid list changes. Rule: every address loopback, no wildcard."
    echo "app pid: $pid; launch epoch_ms: $(cat "$OUT/launch.t0")"
  } >"$slog"
  for i in $(seq 1 "$IDLE_SECONDS"); do
    tick=$((t0 + i * 1000))
    sleep_until_ms "$tick"
    t="$(now_ms)"
    [ $((t - tick)) -le 1000 ] || late=$((late + 1))
    if [ "$(app_pid)" != "$pid" ]; then
      echo "sample $i t=$((t - t0))ms $(iso): the app process $pid is gone (it exited or was closed during the run)" >>"$slog"
      fail "launch $n, sample $i: the app process $pid ended during the idle period (not a socket violation; nothing was written as a pass)"
    fi
    pids="$(all_pids "$pid")"
    raw="$(lsof_raw "$pids")"
    {
      echo "sample $i t=$((t - t0))ms $(iso) pids=$pids"
      if [ "$pids" != "$prev_pids" ]; then
        ps -o pid=,ppid=,comm= -p "$pids" | sed 's/^/  proc /' | redact
        prev_pids="$pids"
      fi
      if [ -n "$raw" ]; then echo "$raw" | sed 's/^/  lsof /'; else echo "  lsof (no socket)"; fi
    } >>"$slog"
    samples=$((samples + 1))
    lines="$(echo "$raw" | socket_violations || true)"
    if [ -n "$lines" ]; then
      echo "VIOLATION: $lines" >>"$slog"
      echo "$lines"
      stop_app
      fail "launch $n, sample $i: a socket is not loopback only"
    fi
    total_sockets=$((total_sockets + $(echo "$raw" | grep -c '^n' || true)))
  done
  {
    echo "app process: $pid; children: $(pgrep -P "$pid" | tr '\n' ' ' | sed 's/ $//'); WebKit helpers started after launch: $(helper_pids | tr '\n' ' ' | sed 's/ $//')"
    echo "RESULT: $samples samples in $(( ($(now_ms) - t0) / 1000 )) s ($late late by more than 1 s), pids $pids, sockets seen: $total_sockets, no wildcard listener and no non-loopback address"
  } >>"$slog"
  grep -E '^(app process|RESULT)' "$slog"

  {
    echo "# Launch $n: app process memory after ${IDLE_SECONDS} s idle, $RSS_SAMPLES samples 1 s apart ($(iso))"
    echo "# ps -o rss= -p <pid>, KB. The app process only; the WebKit helper processes are not included."
  } >"$rlog"
  t0="$(now_ms)"
  for i in $(seq 1 "$RSS_SAMPLES"); do
    sleep_until_ms $((t0 + (i - 1) * 1000))
    echo "t=$(($(now_ms) - t0))ms $(iso) pid=$pid rss_kb=$(ps -o rss= -p "$pid" | tr -d ' ' || true)" >>"$rlog"
  done
  assert_rss_file "$rlog" || {
    stop_app
    fail "launch $n: fewer than $RSS_SAMPLES memory samples"
  }
  values="$(rss_values "$rlog")"
  echo "RESULT: launch $n, $(echo "$values" | wc -l | tr -d ' ') samples: $(echo "$values" | tr '\n' ' ')median $(echo "$values" | median) KB, min $(echo "$values" | min_of) KB, max $(echo "$values" | max_of) KB" >>"$rlog"
  grep '^RESULT' "$rlog"
  stop_app
}

step_idle() {
  local n all=""
  for n in $(seq 1 "$IDLE_LAUNCHES"); do
    idle_launch "$n"
    all="$all$(rss_values "$EVID/04-rss-launch$n.txt")"$'\n'
  done
  all="$(echo "$all" | grep '^[0-9]')"
  {
    echo "# Memory across $IDLE_LAUNCHES launches ($(iso))"
    echo "RESULT: $(echo "$all" | wc -l | tr -d ' ') samples over $IDLE_LAUNCHES launches: median $(echo "$all" | median) KB, min $(echo "$all" | min_of) KB, max $(echo "$all" | max_of) KB, spread $(($(echo "$all" | max_of) - $(echo "$all" | min_of))) KB"
    echo "Memory is a baseline (SPEC section 5): no pass line is set; the readings exist and the spread is printed."
  } >"$EVID/04-rss-all-launches.txt"
  cat "$EVID/04-rss-all-launches.txt"
  {
    echo "# What the socket and memory checks cover and miss"
    echo "covered by the socket check: the app process, its children and the WebKit helper processes that appeared after the launch."
    echo "not attributable: a helper process that exists before the launch (it serves another app); a helper that another app starts during the run (counted here, which only makes the check stricter)."
    echo "not seen: a connection that opens and closes between two 1 s samples."
    echo "memory covers the app process only, not the helper processes."
  } >"$EVID/03-limits.txt"
  cat "$EVID/03-limits.txt"
}

# ------------------------------------------------------------ negative controls

# control <name> <expected: refuse|accept> <exit code> <output>: records and checks one control.
CONTROL_N=0
CONTROL_BAD=0
control_record() { # name expected rc output
  local name="$1" expected="$2" rc="$3" outp="$4" verdict
  CONTROL_N=$((CONTROL_N + 1))
  if { [ "$expected" = "refuse" ] && [ "$rc" -ne 0 ]; } || { [ "$expected" = "accept" ] && [ "$rc" -eq 0 ]; }; then
    verdict="behaved as expected ($expected, exit $rc)"
  else
    verdict="WRONG (expected $expected, exit $rc)"
    CONTROL_BAD=$((CONTROL_BAD + 1))
  fi
  {
    echo "CONTROL $CONTROL_N: $name"
    echo "  expected: $expected"
    echo "  exit: $rc"
    echo "$outp" | sed 's/^/  output: /' | cut -c1-300
    echo "  verdict: $verdict"
    echo
  } >>"$EVID/05-negative-controls.txt"
  echo "control $CONTROL_N: $name: $verdict"
}

serve_listener() { # address -> prints "<pid> <port>"
  python3 -I -c '
import os, socket, sys, time
family = socket.AF_INET6 if ":" in sys.argv[1] else socket.AF_INET
s = socket.socket(family)
s.bind((sys.argv[1], 0))
s.listen()
print(os.getpid(), s.getsockname()[1], flush=True)
time.sleep(25)
' "$1"
}

step_controls() {
  local out="$EVID/05-negative-controls.txt" rc outp bin_line
  {
    echo "# Negative controls: each check is run on input that it must refuse, and on input that it must accept ($(iso))."
    echo "# A check that cannot fail is not evidence."
    echo "# The profile controls use the real rustc line of 01-build-flags.txt with one flag changed: a real build with a"
    echo "# wrong profile would rebuild the whole dependency tree. The override guard is run for real."
    echo
  } >"$out"
  CONTROL_N=0
  CONTROL_BAD=0

  # 1. The override guard.
  rc=0
  outp="$(CARGO_PROFILE_RELEASE_OPT_LEVEL=1 bash "$SELF" override-guard 2>&1)" || rc=$?
  control_record "override guard with CARGO_PROFILE_RELEASE_OPT_LEVEL=1 in the environment" refuse "$rc" "$outp"
  rc=0
  outp="$(env -u CARGO_PROFILE_RELEASE_OPT_LEVEL bash "$SELF" override-guard 2>&1)" || rc=$?
  control_record "override guard in a clean environment" accept "$rc" "$outp"

  # 2. The rustc flags.
  bin_line="$(sed -n 's/^COMMAND: //p' "$EVID/01-build-flags.txt" | grep -- '--crate-type bin' | head -1)"
  [ -n "$bin_line" ] || fail "01-build-flags.txt has no bin command line (run the build-proof step first)"
  rc=0
  outp="$(assert_rustc_line "$bin_line" bin 2>&1)" || rc=$?
  control_record "rustc flags, the real bin line" accept "$rc" "$outp"
  rc=0
  outp="$(assert_rustc_line "${bin_line//-C opt-level=3/-C opt-level=1}" bin 2>&1)" || rc=$?
  control_record "rustc flags with -C opt-level=1" refuse "$rc" "$outp"
  rc=0
  outp="$(assert_rustc_line "${bin_line//-C codegen-units=1/-C codegen-units=16}" bin 2>&1)" || rc=$?
  control_record "rustc flags with -C codegen-units=16" refuse "$rc" "$outp"
  rc=0
  outp="$(assert_rustc_line "${bin_line//-C lto=thin/-C lto=off}" bin 2>&1)" || rc=$?
  control_record "rustc flags with -C lto=off" refuse "$rc" "$outp"
  rc=0
  outp="$(assert_rustc_line "${bin_line//-C strip=symbols/}" bin 2>&1)" || rc=$?
  control_record "rustc flags without a strip flag" refuse "$rc" "$outp"
  rc=0
  outp="$(assert_rustc_line "$bin_line -C panic=abort" bin 2>&1)" || rc=$?
  control_record "rustc flags with -C panic=abort" refuse "$rc" "$outp"

  # 3. The socket rule, on real listeners.
  local tmp lpid lport raw kind
  tmp="$(mktemp -d "$OUT/controlsXXXX")"
  ( serve_listener 0.0.0.0 >"$tmp/wild4" ) &
  disown
  ( serve_listener 127.0.0.1 >"$tmp/loop4" ) &
  disown
  ( serve_listener :: >"$tmp/wild6" ) &
  disown
  sleep 2
  for kind in loop4 wild4 wild6; do
    lpid="$(cut -d' ' -f1 "$tmp/$kind")"
    lport="$(cut -d' ' -f2 "$tmp/$kind")"
    raw="$(lsof_raw "$lpid")"
    outp="$(echo "$raw" | socket_violations)"
    rc=0
    [ -z "$outp" ] || rc=1
    case "$kind" in
      loop4) control_record "socket rule on a test listener bound to 127.0.0.1:$lport (lsof: $(echo "$raw" | grep '^n' | tr '\n' ' '))" accept "$rc" "${outp:-no violation}" ;;
      wild4) control_record "socket rule on a test listener bound to 0.0.0.0:$lport (lsof: $(echo "$raw" | grep '^n' | tr '\n' ' '))" refuse "$rc" "$outp" ;;
      wild6) control_record "socket rule on a test listener bound to [::]:$lport (lsof: $(echo "$raw" | grep '^n' | tr '\n' ' '))" refuse "$rc" "$outp" ;;
    esac
    kill "$lpid" 2>/dev/null || true
  done

  # 4. The window rule.
  rc=0
  outp="$(assert_window "Interlo" 1200 780 1324 2>&1)" || rc=$?
  control_record "window rule: Interlo, 1200 by 780, 1324 ms" accept "$rc" "${outp:-within the limits}"
  rc=0
  outp="$(assert_window "Interlo" 1000 700 1324 2>&1)" || rc=$?
  control_record "window rule: size 1000 by 700" refuse "$rc" "$outp"
  rc=0
  outp="$(assert_window "Other" 1200 780 1324 2>&1)" || rc=$?
  control_record "window rule: title Other" refuse "$rc" "$outp"
  rc=0
  outp="$(assert_window "Interlo" 1200 780 61000 2>&1)" || rc=$?
  control_record "window rule: 61000 ms" refuse "$rc" "$outp"
  rc=0
  outp="$(assert_window "" 1200 780 1324 2>&1)" || rc=$?
  control_record "window rule: empty title is BLOCKED (exit 3), not a pass" refuse "$rc" "$outp"

  # 5. The memory rule.
  printf 'rss_kb=1\nrss_kb=2\nrss_kb=3\n' >"$tmp/short.txt"
  printf 'rss_kb=x\nrss_kb=\nrss_kb=1\nrss_kb=2\nrss_kb=3\n' >"$tmp/garbled.txt"
  printf 'rss_kb=1\nrss_kb=2\nrss_kb=3\nrss_kb=4\nrss_kb=5\n' >"$tmp/ok.txt"
  rc=0
  outp="$(assert_rss_file "$tmp/ok.txt" 2>&1)" || rc=$?
  control_record "memory rule: 5 readings" accept "$rc" "${outp:-5 readings}"
  rc=0
  outp="$(assert_rss_file "$tmp/short.txt" 2>&1)" || rc=$?
  control_record "memory rule: 3 readings" refuse "$rc" "${outp//$tmp\//}"
  rc=0
  outp="$(assert_rss_file "$tmp/garbled.txt" 2>&1)" || rc=$?
  control_record "memory rule: 3 numbers and 2 broken lines" refuse "$rc" "${outp//$tmp\//}"

  # 6. The blocked path: a missing identity is BLOCKED, never a pass.
  rc=0
  outp="$(APP_CHECK_IDENTITY="No Such Identity" APP_CHECK_EVIDENCE="$tmp/evidence" bash "$SELF" prereq 2>&1)" || rc=$?
  control_record "prerequisites with a signing identity that does not exist (BLOCKED is exit 3)" refuse "$rc" "$outp"
  [ "$rc" -eq 3 ] || { echo "the missing identity did not give exit 3 (BLOCKED)"; CONTROL_BAD=$((CONTROL_BAD + 1)); }

  find "$tmp" -type f -delete
  find "$tmp" -type d -empty -delete 2>/dev/null || true
  {
    echo "RESULT: $CONTROL_N controls, $CONTROL_BAD wrong"
  } >>"$out"
  [ "$CONTROL_BAD" -eq 0 ] || fail "$CONTROL_BAD negative controls did not behave as expected (see 05-negative-controls.txt)"
  echo "RESULT: $CONTROL_N controls, 0 wrong"
}

# --------------------------------------------------------- summary and checksums

step_evidence() {
  local sum="$EVID/SUMMARY.md" f commit date
  commit="$(sed -n 's/^commit: //p' "$EVID/meta.txt")"
  date="$(sed -n 's/^date: //p' "$EVID/meta.txt")"
  [ -f "$EVID/05-negative-controls.txt" ] || fail "run the controls step first"

  # Control for the checksum file: a changed file must break `shasum -c`.
  local tmp rc=0 outp
  tmp="$(mktemp -d "$OUT/sumcheckXXXX")"
  cp "$EVID/00-bundle.txt" "$tmp/file.txt"
  (cd "$tmp" && shasum -a 256 file.txt >SHA256SUMS)
  (cd "$tmp" && shasum -a 256 -c SHA256SUMS >/dev/null 2>&1) || fail "the checksum control: a clean copy did not verify"
  printf 'x' >>"$tmp/file.txt"
  outp="$(cd "$tmp" && shasum -a 256 -c SHA256SUMS 2>&1)" || rc=$?
  {
    echo "# Control of the checksum file ($(iso)): a file changed by one byte must fail shasum -c"
    echo "$outp"
    echo "exit: $rc"
    echo "RESULT: $([ "$rc" -ne 0 ] && echo 'the changed file was refused' || echo 'WRONG: the changed file was accepted')"
  } >"$EVID/06-checksum-control.txt"
  find "$tmp" -type f -delete
  find "$tmp" -type d -empty -delete 2>/dev/null || true
  [ "$rc" -ne 0 ] || fail "the checksum control accepted a changed file"

  {
    echo "# Evidence summary"
    echo
    echo "Commit measured: \`$commit\`. Date: $date. Command: \`scripts/check.sh app\`."
    echo "The raw files in this folder are the evidence; this page only quotes them. \`SHA256SUMS\` lists every other file."
    echo "Verify: \`cd docs/m0/M0b-01/evidence && shasum -a 256 -c SHA256SUMS\`."
    echo
    echo "## Quoted result lines"
    echo
    for f in 00-bundle.txt 01-build-flags.txt 02-window-poll.log 03-sockets-launch1.log 03-sockets-launch2.log 03-sockets-launch3.log 04-rss-launch1.txt 04-rss-launch2.txt 04-rss-launch3.txt 04-rss-all-launches.txt 05-negative-controls.txt 06-checksum-control.txt; do
      echo "- \`$f\`:"
      grep -E '^(RESULT|exit: 0|FLAGS)' "$EVID/$f" | sed 's/^/  > /' || true
    done
    echo
    echo "## Negative controls (05-negative-controls.txt)"
    echo
    grep -E '^CONTROL|verdict:' "$EVID/05-negative-controls.txt" | paste - - | sed 's/\t  verdict:/  ->/' | sed 's/^/- /'
    echo
    echo "## Limits"
    echo
    sed 's/^# //; s/^/- /' "$EVID/03-limits.txt" | tail -n +2
    echo "- The screenshots show the window at two moments (the first poll that found it, and 5 s later); they are not a recording."
    echo "- Rerun \`scripts/check.sh app\` to compare: the pass lines must repeat, the memory numbers vary (read the median and the spread)."
  } >"$sum"

  (cd "$EVID" && find . -type f ! -name SHA256SUMS | sort | sed 's#^\./##' | while IFS= read -r f; do shasum -a 256 "$f"; done >SHA256SUMS)
  (cd "$EVID" && shasum -a 256 -c SHA256SUMS >"$OUT/sums-check.txt" 2>&1) || fail "SHA256SUMS does not verify"
  echo "SUMMARY.md written; SHA256SUMS covers $(wc -l <"$EVID/SHA256SUMS" | tr -d ' ') files and verifies"
  du -sh "$EVID" | sed "s#$ROOT/##"
}

main() {
  case "${1:-}" in
    prereq) step_prereq ;;
    override-guard) step_override_guard ;;
    build-proof) step_build_proof ;;
    bundle) step_bundle ;;
    window) step_window ;;
    idle) step_idle ;;
    controls) step_controls ;;
    evidence) step_evidence ;;
    *)
      echo "usage: scripts/app-check.sh prereq|build-proof|bundle|window|idle|controls|evidence|override-guard" >&2
      exit 64
      ;;
  esac
}

# Sourced by scripts/app-check.test.sh: define the functions and stop.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
