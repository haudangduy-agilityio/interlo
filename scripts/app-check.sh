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
#   scripts/app-check.sh telex           OPT-IN, off by default (scripts/check.sh app --typing): types a sentence into the
#                                        note field through the built-in Vietnamese (Telex) input source with synthetic key
#                                        events. It needs Accessibility for the app that runs it and the Telex input source
#                                        enabled, and it changes the input source and the clipboard for about a minute.
#                                        SIGINT and SIGTERM restore both; SIGKILL cannot be trapped: the recovery command
#                                        is in its log and in target/app-check/typing-recovery.txt.
#   scripts/app-check.sh controls        negative controls: each check must be able to fail
#   scripts/app-check.sh evidence        SUMMARY.md and SHA256SUMS
#   scripts/app-check.sh override-guard  refuses an environment that overrides the release profile
#
# Evidence: every step writes its raw output into $APP_CHECK_EVIDENCE (default
# docs/m0/M0b-01/evidence). Nothing is summarised away: the raw files carry the
# timestamps, pids and command lines, SUMMARY.md only quotes them and SHA256SUMS
# covers every file. The repository evidence folder holds text only. Screenshots
# (PNGs of the Interlo window) go to $APP_CHECK_SHOTS, a folder outside the
# repository (default: ../interlo-evidence/M0b-01 next to the repository); their
# hashes and local paths are listed in SHA256SUMS. A step fails when a file that is
# not text appears under the evidence folder.
#
# Exit codes: 0 pass, 1 fail, 3 BLOCKED (a tool, a permission or an identity is
# missing; the environment does not provide it). A blocked step is never a pass.
# Written for bash 3.2 (the macOS default): no associative arrays, no mapfile.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${APP_CHECK_DIR:-$ROOT/target/app-check}"
EVID="${APP_CHECK_EVIDENCE:-$ROOT/docs/m0/M0b-01/evidence}"
SHOTS="${APP_CHECK_SHOTS:-$(cd "$ROOT/.." && pwd)/interlo-evidence/M0b-01}"
APP="$ROOT/target/release/bundle/macos/Interlo.app"
BIN="$APP/Contents/MacOS/interlo-desktop"
IDENTITY="${APP_CHECK_IDENTITY:-Interlo Local Signing}"
HELPER_SRC="$ROOT/scripts/app-check/window-info.swift"
HELPER="$OUT/window-info"
INPUT_SOURCE="$OUT/input-source"
DRIVE="$OUT/app-drive"
TELEX_SOURCE="com.apple.inputmethod.VietnameseIM.VietnameseTelex"
SENTENCE="Hôm nay mình rảnh, cũng gặp nhau ở quán bạn nhé."
# The Telex spelling of the sentence, and the same keys with one wrong tone key (s, not f, after "minh").
TELEX_KEYS="Hoom nay minhf ranhr, cungx gawpj nhau owr quans banj nhes."
TELEX_KEYS_WRONG_TONE="Hoom nay minhs ranhr, cungx gawpj nhau owr quans banj nhes."
KEY_DELAY_MS=120
TYPING_ATTEMPTS=8
TAURI_MANIFEST="apps/desktop/src-tauri/Cargo.toml"
SELF="$ROOT/scripts/app-check.sh"

# The limits are starting guesses (window within 60 s, 1200 by 780 points). Memory
# gets no pass line because it is a baseline: the readings must exist and their
# spread is printed.
WINDOW_TITLE="Interlo"
WINDOW_WIDTH=1200
WINDOW_HEIGHT=780
WINDOW_DEADLINE_MS=60000
SHOT_LATE_AFTER_S=5
IDLE_SECONDS=60
IDLE_LAUNCHES=3
RSS_SAMPLES=5

trap 'echo "internal error: line $LINENO exited with $?" >&2' ERR

# Exit codes: 3 BLOCKED (the environment does not provide a tool, permission or state), 1 fail.
# app-drive uses 3 for a missing Accessibility grant and 4 when the app is not in front;
# idle_launch returns 5 when the app process ends by itself.
EXIT_BLOCKED=3
EXIT_NOT_FRONT=4
EXIT_APP_ENDED=5

# fail and blocked write to stderr: they are also called inside $( ), where stdout is captured.
blocked() {
  echo "BLOCKED: $1" >&2
  exit "$EXIT_BLOCKED"
}

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'; }
iso() { date '+%Y-%m-%dT%H:%M:%S%z'; }
# Sleep until the given epoch millisecond (returns at once when it has passed).
sleep_until_ms() { perl -MTime::HiRes=sleep,time -e 'my $d = $ARGV[0] / 1000 - time; sleep($d) if $d > 0' "$1"; }
# Shortens the home directory to ~ in the command lines and logs that pass through it
# (the evidence files also carry the local screenshot paths, which name the user).
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

# True when the `ioreg -n Root -d1 -a` text on stdin says that the screen is locked.
screen_is_locked() {
  grep -A1 'CGSSessionScreenIsLocked' | grep -q '<true/>'
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
  grep -qE -- '-C opt-level=3( |$)' <<<"$line" || { echo "$kind target: no -C opt-level=3"; return 1; }
  grep -qE -- '-C codegen-units=1( |$)' <<<"$line" || { echo "$kind target: no -C codegen-units=1"; return 1; }
  grep -qE -- '-C strip=(symbols|debuginfo)( |$)' <<<"$line" || { echo "$kind target: no -C strip flag"; return 1; }
  if grep -qE -- '-C panic=' <<<"$line"; then
    echo "$kind target: a -C panic flag is set (unwinding must stay the default)"
    return 1
  fi
  if [ "$kind" = "bin" ]; then
    grep -qE -- '-C lto=thin( |$)' <<<"$line" || { echo "bin target: no -C lto=thin"; return 1; }
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

# Compares two UTF-8 files after Unicode normalisation to NFC. Prints a diff of the
# normalised texts when they differ and returns 1; prints nothing and returns 0 when equal.
nfc_diff() { # file_a file_b
  local a b rc=0
  a="$(mktemp "${TMPDIR:-/tmp}/nfc-aXXXXXX")"
  b="$(mktemp "${TMPDIR:-/tmp}/nfc-bXXXXXX")"
  python3 -I -c 'import sys, unicodedata
for source, target in ((sys.argv[1], sys.argv[3]), (sys.argv[2], sys.argv[4])):
    data = open(source, encoding="utf-8").read()
    open(target, "w", encoding="utf-8").write(unicodedata.normalize("NFC", data))' "$1" "$2" "$a" "$b"
  diff "$a" "$b" || rc=$?
  rm -f "$a" "$b"
  return "$rc"
}

# Prints every file under the directory that is not text (an image, audio, a binary).
# Prints nothing and returns 0 when the directory holds text only.
non_text_files() { # dir
  local f mime found=1
  while IFS= read -r f; do
    mime="$(file -b --mime-type "$f")"
    case "$mime" in
      text/* | inode/x-empty) ;;
      *)
        echo "$f ($mime)"
        found=0
        ;;
    esac
  done < <(find "$1" -type f | sort)
  [ "$found" -ne 0 ]
}

# Fails (returns 1) when the directory holds a file that is not text.
assert_text_only() { # dir
  local list
  list="$(non_text_files "$1" || true)"
  if [ -n "$list" ]; then
    echo "the evidence folder must hold text only; found: $list"
    return 1
  fi
  return 0
}

rss_values() { sed -n 's/.*rss_kb=\([0-9][0-9]*\)$/\1/p' "$1"; }

# ------------------------------------------------------------------ process helpers

# Pid of the process whose command is exactly the app binary.
app_pids() {
  ps -axo pid=,command= | awk -v b="$BIN" '{ pid = $1; $1 = ""; sub(/^ /, ""); if ($0 == b) print pid }'
}

app_pid() { app_pids | sed -n 1p; }

# Ends every process whose command is exactly the app binary, by pid. The path is
# compared as a string and never used as a pattern, so another worktree's app is safe.
stop_app() {
  local pid i
  for pid in $(app_pids); do kill "$pid" 2>/dev/null || true; done
  for i in 1 2 3 4 5 6 7 8 9 10; do
    [ -z "$(app_pid)" ] && return 0
    sleep 0.5
  done
  for pid in $(app_pids); do kill -9 "$pid" 2>/dev/null || true; done
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
# lsof exits 1 and prints nothing when no socket matches. Whatever it writes to stderr
# is kept in $OUT/lsof.err, so a failed lsof is not read as "no socket": the callers
# log it, and the idle check fails when lsof reports an error.
lsof_raw() { lsof -nP -a -p "$1" -i -F pPn 2>"$OUT/lsof.err" || true; }

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
  # A locked screen hides the windows: the window, memory and screenshot checks cannot run.
  local ioreg_text
  ioreg_text="$(ioreg -n Root -d1 -a 2>&1)" || blocked "ioreg failed, so the lock state of the screen is unknown: $ioreg_text"
  if screen_is_locked <<<"$ioreg_text"; then
    blocked "the screen is locked (CGSSessionScreenIsLocked is true): unlock the Mac and run the check again"
  fi
  # The step deletes the files under the evidence folder, so it refuses a folder that is not one:
  # the path is checked before anything is created. It must end in /evidence and sit under
  # docs/m0/ of the repository or under the scratch folder, and the screenshots must stay outside git.
  local root_real evid_abs shots_abs
  root_real="$(cd "$ROOT" && pwd -P)"
  evid_abs="$(cd "$(dirname "$EVID")" 2>/dev/null && pwd -P)/$(basename "$EVID")" || fail "the parent of APP_CHECK_EVIDENCE does not exist"
  shots_abs="$(cd "$(dirname "$SHOTS")" 2>/dev/null && pwd -P)/$(basename "$SHOTS")" || fail "the parent of APP_CHECK_SHOTS does not exist"
  [ "$(basename "$evid_abs")" = "evidence" ] || fail "APP_CHECK_EVIDENCE ($evid_abs) must be a folder named evidence"
  case "$evid_abs" in
    "$root_real"/docs/m0/*) ;;
    "$root_real"/target/*) ;;
    *) fail "APP_CHECK_EVIDENCE ($evid_abs) must be under docs/m0/ or under target/ of the repository" ;;
  esac
  case "$shots_abs" in
    "$root_real" | "$root_real"/*) fail "APP_CHECK_SHOTS ($shots_abs) is inside the repository: screenshots must stay outside git" ;;
  esac
  mkdir -p "$OUT" "$EVID" "$SHOTS"
  [ ! -e "$EVID/.git" ] || fail "APP_CHECK_EVIDENCE holds a .git entry"
  # A new run replaces the evidence of the last run.
  find "$EVID" -type f -delete
  : >"$OUT/shots.list"
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
    echo "screenshots are stored outside the repository: $SHOTS"
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
  local summary="" seen_lib=0 seen_bin=0
  while IFS= read -r line; do
    n=$((n + 1))
    kind="$(echo "$line" | grep -oE -- '--crate-type [a-z]+' | cut -d' ' -f2)"
    local flags
    flags="$(echo "$line" | grep -oE -- '-C (opt-level|codegen-units|lto|linker-plugin-lto|strip|panic)(=[a-z0-9]+)?' | tr '\n' ' ')"
    echo "FLAGS $kind: $flags" >>"$out"
    assert_rustc_line "$line" "$kind" >>"$out" || {
      tail -3 "$out"
      fail "$kind target: the rustc flags do not match the profile"
    }
    summary="$summary$kind "
    case "$kind" in lib) seen_lib=1 ;; bin) seen_bin=1 ;; esac
  done <<EOF
$lines
EOF
  [ "$seen_lib" -eq 1 ] && [ "$seen_bin" -eq 1 ] || fail "expected a lib and a bin rustc command line, found: ${summary:-none}"
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
  # The signing and notarization variables of the Tauri bundler are removed: this build is signed locally only.
  env -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID -u APPLE_SIGNING_IDENTITY -u APPLE_CERTIFICATE \
    -u APPLE_CERTIFICATE_PASSWORD -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH -u APPLE_PROVIDER_SHORT_NAME \
    -u TAURI_SIGNING_PRIVATE_KEY -u TAURI_SIGNING_PRIVATE_KEY_PASSWORD -u TAURI_SIGNING_PRIVATE_KEY_PATH \
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
  local log="$EVID/02-window-poll.log" pid start now elapsed info="" first="" id title w h epoch first_shot late_shot
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
  first_shot="$SHOTS/first-frame-win${id}-t${elapsed}ms-epoch${epoch}.png"
  local try=0
  until screencapture -x -o -l "$id" "$first_shot" 2>/dev/null; do
    try=$((try + 1))
    [ "$try" -lt 10 ] || fail "screencapture of window $id failed 10 times in a row (is the screen locked or asleep?)"
    sleep 0.2
  done
  echo "$first_shot" >>"$OUT/shots.list"
  echo "capture: first-frame-win${id}-t${elapsed}ms-epoch${epoch}.png taken at t=${elapsed}ms (the first poll that found the window)" >>"$log"
  sleep "$SHOT_LATE_AFTER_S"
  now="$(now_ms)"
  epoch="$(date +%s)"
  local late=$((now - start))
  late_shot="$SHOTS/after-${SHOT_LATE_AFTER_S}s-win${id}-t${late}ms-epoch${epoch}.png"
  screencapture -x -o -l "$id" "$late_shot" || fail "second screencapture failed"
  echo "$late_shot" >>"$OUT/shots.list"
  echo "capture: after-${SHOT_LATE_AFTER_S}s-win${id}-t${late}ms-epoch${epoch}.png taken at t=${late}ms" >>"$log"
  stop_app
  local shot
  for shot in "$first_shot" "$late_shot"; do
    [ -s "$shot" ] || fail "$shot is empty"
    echo "$(basename "$shot"): $(sips -g pixelWidth -g pixelHeight "$shot" | awk '/pixel/ { printf "%s ", $2 }')pixels, $(wc -c <"$shot" | tr -d ' ') bytes" >>"$log"
  done
  local rc=0 msg
  msg="$(assert_window "$title" "$w" "$h" "$elapsed")" || rc=$?
  if [ "$rc" -eq 3 ]; then blocked "$msg"; fi
  [ "$rc" -eq 0 ] || fail "$msg"
  echo "RESULT: title \"$title\", size ${w} by ${h} points, found ${elapsed} ms after launch (limit ${WINDOW_DEADLINE_MS} ms); two screenshots of window $id" >>"$log"
  grep -E '^(capture|RESULT)|pixels' "$log"
  echo "screenshots are stored outside the repository, in $SHOTS"
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
      echo "launch $n: the app process $pid ended at sample $i of the idle period (not a socket violation; no crash report was looked up here)"
      return "$EXIT_APP_ENDED"
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
      if [ -s "$OUT/lsof.err" ]; then sed 's/^/  lsof stderr: /' "$OUT/lsof.err" | redact; fi
    } >>"$slog"
    if [ -s "$OUT/lsof.err" ]; then
      stop_app
      fail "launch $n, sample $i: lsof wrote to stderr, so an empty reading cannot be read as \"no socket\" (see the log)"
    fi
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
  if [ "$(app_pid)" != "$pid" ]; then
    echo "the app process $pid is gone before the memory samples were complete" >>"$rlog"
    return "$EXIT_APP_ENDED"
  fi
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
  local attempt rc interrupted="$EVID/03-interrupted-launches.log"
  for n in $(seq 1 "$IDLE_LAUNCHES"); do
    # A launch whose app process ends by itself is recorded and repeated (at most 3 tries):
    # an app that ends every time still fails the step.
    for attempt in 1 2 3; do
      rc=0
      idle_launch "$n" || rc=$?
      [ "$rc" -eq "$EXIT_APP_ENDED" ] || break
      stop_app
      # Keep the partial logs of the interrupted try: the repeat writes the same file names.
      cp "$EVID/03-sockets-launch$n.log" "$EVID/03-sockets-launch$n-try$attempt-interrupted.log"
      [ ! -f "$EVID/04-rss-launch$n.txt" ] || mv "$EVID/04-rss-launch$n.txt" "$EVID/04-rss-launch$n-try$attempt-interrupted.txt"
      echo "RESULT: launch $n, try $attempt: the app process ended before the readings were complete ($(iso)); the launch was repeated; partial logs: 03-sockets-launch$n-try$attempt-interrupted.log" >>"$interrupted"
      [ "$attempt" -lt 3 ] || fail "launch $n: the app process ended on 3 tries in a row (see 03-interrupted-launches.log)"
    done
    [ "$rc" -eq 0 ] || fail "launch $n failed (exit $rc)"
    all="$all$(rss_values "$EVID/04-rss-launch$n.txt")"$'\n'
  done
  all="$(echo "$all" | grep '^[0-9]')"
  {
    echo "# Memory across $IDLE_LAUNCHES launches ($(iso))"
    echo "RESULT: $(echo "$all" | wc -l | tr -d ' ') samples over $IDLE_LAUNCHES launches: median $(echo "$all" | median) KB, min $(echo "$all" | min_of) KB, max $(echo "$all" | max_of) KB, spread $(($(echo "$all" | max_of) - $(echo "$all" | min_of))) KB"
    echo "Memory is a baseline: no pass line is set; the readings exist and the spread is printed."
  } >"$EVID/04-rss-all-launches.txt"
  cat "$EVID/04-rss-all-launches.txt"
  {
    echo "# What the socket and memory checks cover and miss"
    echo "covered by the socket check: the app process, its children and the WebKit helper processes that appeared after the launch."
    echo "not attributable: a helper process that exists before the launch (it serves another app); a helper that another app starts during the run (counted here, which only makes the check stricter)."
    echo "not seen: a connection that opens and closes between two 1 s samples; DNS lookups done for the app by a system daemon; network work done for the app by another process; Unix-domain sockets (lsof -i lists internet sockets only)."
    echo "scope: the app idle on its first screen for 60 s, three launches. This is evidence about the app process tree and its web view helpers while idle: not a proof that nothing leaves the Mac, and not a reading during a meeting."
    echo "a helper process that another app starts during the run is counted too: if it has a non-loopback socket the check fails and its address is written to the log, so close other apps that use a web view while this runs."
    echo "memory covers the app process only, not the helper processes."
    if [ -s "$interrupted" ]; then
      echo "interrupted launches: $(wc -l <"$interrupted" | tr -d ' ') launch(es) ended by themselves and were repeated (see 03-interrupted-launches.log); the cause was not found."
    else
      echo "interrupted launches: none."
    fi
  } >"$EVID/03-limits.txt"
  cat "$EVID/03-limits.txt"
  if [ -s "$interrupted" ]; then
    echo "INTERRUPTED LAUNCHES: $(wc -l <"$interrupted" | tr -d ' ') (the app process ended by itself and the launch was repeated; see 03-interrupted-launches.log; the cause was not found)"
  fi
}

# -------------------------------------------------------------- Telex typing

# Presses the named element, waiting up to 60 s for the page to show it.
press_when_ready() { # pid text
  local i rc=1
  for i in $(seq 1 120); do
    rc=0
    "$DRIVE" press "$1" "$2" || rc=$?
    [ "$rc" -eq 0 ] && return 0
    sleep 0.5
  done
  return "$rc"
}

# Types KEYS into the note field with synthetic key events posted to the app's pid
# (they cannot reach another app). The input method composes only while the app is
# in front, so the typing is repeated when another app takes the front during it
# (exit 4 of app-drive). Returns 0 when the keys were typed, EXIT_BLOCKED when
# Accessibility is missing or the app could not be held in front, 1 on any other failure.
type_into_field() { # pid keys log
  local pid="$1" keys="$2" log="$3" attempt rc
  for attempt in $(seq 1 "$TYPING_ATTEMPTS"); do
    rc=0
    {
      echo "attempt $attempt ($(iso))"
      press_when_ready "$pid" Workspace && sleep 0.5 &&
        press_when_ready "$pid" Notes && sleep 0.5 &&
        "$DRIVE" raise "$pid" bounce &&
        "$DRIVE" focus-text "$pid" &&
        "$DRIVE" chord "$pid" select-all &&
        "$DRIVE" chord "$pid" delete &&
        "$DRIVE" keys "$pid" "$KEY_DELAY_MS" "$keys"
    } >>"$log" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
      sleep 0.7
      echo "  field after typing (read with the Accessibility API): [$("$DRIVE" read-text "$pid" | head -c 300)]" >>"$log"
      return 0
    fi
    echo "  attempt $attempt stopped with exit $rc ($EXIT_NOT_FRONT means the app was not in front; the keys go to its pid only, the field is cleared and typed again)" >>"$log"
    [ "$rc" -eq "$EXIT_BLOCKED" ] && return "$EXIT_BLOCKED"
    [ "$rc" -eq "$EXIT_NOT_FRONT" ] || return 1
    sleep 1
  done
  return "$EXIT_BLOCKED"
}

# Copies the field with Cmd+A, Cmd+C into the given file. The clipboard holds a
# marker first, so a copy that did not happen shows the marker, not an old text.
copy_field() { # pid out_file log
  local pid="$1" outfile="$2" log="$3" attempt rc
  export LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8
  for attempt in $(seq 1 "$TYPING_ATTEMPTS"); do
    printf '%s' "app-check-clipboard-marker" | pbcopy
    rc=0
    { "$DRIVE" chord "$pid" select-all && "$DRIVE" chord "$pid" copy; } >>"$log" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
      pbpaste >"$outfile"
      return 0
    fi
    [ "$rc" -eq "$EXIT_BLOCKED" ] && return "$EXIT_BLOCKED"
    [ "$rc" -eq "$EXIT_NOT_FRONT" ] || return 1
    sleep 1
  done
  return "$EXIT_BLOCKED"
}

step_telex() {
  export LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8
  local log="$EVID/07-telex-typing.log" pid orig_source saved_clip="$OUT/clipboard.saved" saved_hash rc info id shot
  local field="$EVID/07-field.txt" sentence="$EVID/07-sentence.txt" wrong="$EVID/07-field-wrong-tone.txt" outp
  echo "NOTICE: the next step changes your input source and your clipboard for about a minute, brings the Interlo window to the front and types with synthetic key events. Do not use the keyboard or the mouse until it ends. Only the clipboard TEXT is restored (an image, a file or a concealed password marker is not): do not run it with a secret copied. To stop it, send SIGTERM or SIGINT to the whole process group."
  # Only this opt-in step needs Accessibility and the Telex input source.
  swiftc -O -o "$INPUT_SOURCE" "$ROOT/scripts/app-check/input-source.swift" || blocked "the input source tool did not compile"
  swiftc -O -o "$DRIVE" "$ROOT/scripts/app-check/app-drive.swift" || blocked "the app driver did not compile"
  "$DRIVE" trusted >/dev/null || blocked "Accessibility is not granted to the app that runs this script (AXIsProcessTrusted() is false)"
  "$INPUT_SOURCE" list | grep -q "^$TELEX_SOURCE" || blocked "the input source $TELEX_SOURCE is not enabled (System Settings, Keyboard, Input Sources)"
  # A leftover saved file means an earlier run was killed before it could restore: it is not overwritten.
  # These checks and the save come before the app is launched, so a refusal leaves nothing running.
  if [ -e "$saved_clip" ]; then
    blocked "$saved_clip exists: an earlier typing run did not restore the clipboard. Restore it by hand (pbcopy < that file), then delete the file"
  fi
  if [ "$(pbpaste)" = "app-check-clipboard-marker" ]; then
    blocked "the clipboard holds the marker text of an earlier killed run: copy your own text back first"
  fi
  orig_source="$("$INPUT_SOURCE" current)"
  (umask 077 && pbpaste >"$saved_clip") || blocked "the clipboard could not be saved, so it could not be restored"
  saved_hash="$(shasum -a 256 <"$saved_clip" | cut -d' ' -f1)"
  pid="$(launch_app)"
  # Restores the input source and the clipboard text, and says so in the log when a command fails.
  # The saved copy is deleted only when the clipboard was restored from it.
  restore_state() {
    "$INPUT_SOURCE" select "$orig_source" >/dev/null 2>&1 || echo "restore: selecting $orig_source failed" >>"$log"
    if pbcopy <"$saved_clip" 2>>"$log"; then
      rm -f "$saved_clip" "$OUT/typing-recovery.txt"
    else
      echo "restore: pbcopy failed; the saved text stays in $saved_clip" >>"$log"
    fi
    stop_app
  }
  # SIGINT and SIGTERM restore, then end the step: a trap that only restores would let the typing go on.
  on_signal() { # name exit_code
    echo "signal $1 received: restoring the input source and the clipboard, then stopping" >>"$log"
    restore_state
    trap - EXIT INT TERM
    exit "$2"
  }
  trap restore_state EXIT
  trap 'on_signal INT 130' INT
  trap 'on_signal TERM 143' TERM
  {
    echo "# Typing check (AC-9, AC-10): the sentence is typed with synthetic key events through the built-in Vietnamese (Telex) input source ($(iso))"
    echo "# Nothing is set into the field directly: the keys below go through the system input method into the web view."
    echo "app pid: $pid; input source before: $orig_source; clipboard saved: $(wc -c <"$saved_clip" | tr -d ' ') bytes (text only, not written here)"
    echo "sentence: $SENTENCE"
    echo "keys sent (key delay ${KEY_DELAY_MS} ms): $TELEX_KEYS"
    echo "recovery after a SIGKILL (it cannot be trapped): $INPUT_SOURCE select $orig_source; pbcopy < $saved_clip" | redact
  } >"$log"
  echo "$INPUT_SOURCE select $orig_source; pbcopy < $saved_clip" >"$OUT/typing-recovery.txt"
  # Wait for the window before anything is sent.
  local i
  for i in $(seq 1 600); do
    info="$("$HELPER" "$pid" | sed -n 1p)"
    [ -z "$info" ] || break
    sleep 0.1
  done
  [ -n "$info" ] || fail "the app showed no window"
  id="$(echo "$info" | cut -f1)"
  sleep 2
  "$INPUT_SOURCE" select "$TELEX_SOURCE" >>"$log" 2>&1 || blocked "the input source $TELEX_SOURCE could not be selected"
  echo "input source selected: $("$INPUT_SOURCE" current)" >>"$log"

  # 1. The sentence.
  rc=0
  type_into_field "$pid" "$TELEX_KEYS" "$log" || rc=$?
  [ "$rc" -ne "$EXIT_BLOCKED" ] || blocked "the typing could not run: Accessibility is missing, or another app kept the front during $TYPING_ATTEMPTS attempts (nothing was sent to it); see 07-telex-typing.log"
  [ "$rc" -eq 0 ] || fail "typing stopped (see 07-telex-typing.log)"
  shot="$SHOTS/telex-typed-win${id}-epoch$(date +%s).png"
  if screencapture -x -o -l "$id" "$shot" && [ -s "$shot" ]; then
    echo "$shot" >>"$OUT/shots.list"
  else
    echo "screenshot of the typed field: the capture failed (no file kept)" >>"$log"
  fi
  rc=0
  copy_field "$pid" "$field" "$log" || rc=$?
  [ "$rc" -ne "$EXIT_BLOCKED" ] || blocked "the copy could not run: Accessibility is missing, or another app kept the front; see 07-telex-typing.log"
  [ "$rc" -eq 0 ] || fail "the copy failed (see 07-telex-typing.log)"
  printf '%s' "$SENTENCE" >"$sentence"
  {
    echo "\$ pbpaste > 07-field.txt"
    echo "field.txt:    $(wc -c <"$field" | tr -d ' ') bytes, sha256 $(shasum -a 256 <"$field" | cut -d' ' -f1)"
    echo "sentence.txt: $(wc -c <"$sentence" | tr -d ' ') bytes, sha256 $(shasum -a 256 <"$sentence" | cut -d' ' -f1)"
    echo "\$ diff 07-field.txt 07-sentence.txt"
  } >>"$log"
  rc=0
  outp="$(diff "$field" "$sentence")" || rc=$?
  echo "$outp" | head -20 | cut -c1-300 >>"$log"
  echo "raw diff exit: $rc" >>"$log"
  echo "\$ diff (both files normalised to NFC with python unicodedata)" >>"$log"
  rc=0
  outp="$(nfc_diff "$field" "$sentence")" || rc=$?
  echo "$outp" | head -20 | cut -c1-300 >>"$log"
  echo "NFC diff exit: $rc" >>"$log"
  if [ "$rc" -ne 0 ]; then
    echo "RESULT: NO (AC-10): the field text differs from the sentence; see the diff above" >>"$log"
    grep -E '^(field after typing|RESULT)' "$log"
    fail "AC-9: the text copied from the field is not the sentence (AC-10: result NO)"
  fi
  echo "RESULT: PASS (AC-9): the text copied from the note field equals the sentence, the diff printed nothing" >>"$log"

  # 2. The negative control: one wrong tone key must make the diff report the difference.
  {
    echo
    echo "# Negative control: the same keys with one wrong tone key (s after minh, not f)"
    echo "keys sent: $TELEX_KEYS_WRONG_TONE"
  } >>"$log"
  rc=0
  type_into_field "$pid" "$TELEX_KEYS_WRONG_TONE" "$log" || rc=$?
  [ "$rc" -ne "$EXIT_BLOCKED" ] || blocked "the negative control could not type: Accessibility is missing, or another app kept the front; see 07-telex-typing.log"
  [ "$rc" -eq 0 ] || fail "typing the negative control stopped (see 07-telex-typing.log)"
  rc=0
  copy_field "$pid" "$wrong" "$log" || rc=$?
  [ "$rc" -ne "$EXIT_BLOCKED" ] || blocked "the copy of the negative control could not run: Accessibility is missing, or another app kept the front; see 07-telex-typing.log"
  [ "$rc" -eq 0 ] || fail "the copy of the negative control failed"
  rc=0
  outp="$(nfc_diff "$wrong" "$sentence")" || rc=$?
  {
    echo "\$ diff (NFC) 07-field-wrong-tone.txt 07-sentence.txt"
    echo "$outp" | head -20 | cut -c1-300
    echo "NFC diff exit: $rc"
  } >>"$log"
  if [ "$rc" -eq 0 ]; then
    echo "RESULT: the negative control was NOT detected: the diff printed nothing for a wrong tone" >>"$log"
    fail "the negative control (a wrong tone key) was not detected"
  fi
  echo "RESULT: the negative control was detected: the diff reports the wrong tone (exit $rc)" >>"$log"

  # 3. Put the input source and the clipboard back, then check that they are back.
  restore_state
  trap - EXIT INT TERM
  {
    echo
    echo "# Restore"
    echo "input source after: $("$INPUT_SOURCE" current) (before: $orig_source)"
    if [ "$("$INPUT_SOURCE" current)" = "$orig_source" ] && [ "$(pbpaste | shasum -a 256 | cut -d' ' -f1)" = "$saved_hash" ]; then
      echo "RESULT: the input source and the clipboard text are back as before"
    else
      echo "RESULT: WRONG: the input source or the clipboard was not restored"
    fi
  } >>"$log"
  grep -E '^(field after typing|raw diff|NFC diff|RESULT|  field after)' "$log"
  grep -q 'back as before' "$log" || fail "the input source or the clipboard was not restored (see 07-telex-typing.log; a saved copy of the clipboard text may remain in $saved_clip)"
  echo "typing check: PASS; the screenshot of the typed field is stored outside the repository, in $SHOTS"
}

# ------------------------------------------------------------ negative controls

# control <name> <expected: refuse|accept> <exit code> <output>: records and checks one control.
CONTROL_N=0
CONTROL_BAD=0
# expected is accept (exit 0), refuse (any non-zero exit) or refuse:N (exactly exit N).
control_record() { # name expected rc output
  local name="$1" expected="$2" rc="$3" outp="$4" verdict
  CONTROL_N=$((CONTROL_N + 1))
  local ok=1
  case "$expected" in
    accept) [ "$rc" -eq 0 ] && ok=0 ;;
    refuse) [ "$rc" -ne 0 ] && ok=0 ;;
    refuse:*) [ "$rc" -eq "${expected#refuse:}" ] && ok=0 ;;
  esac
  if [ "$ok" -eq 0 ]; then
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

  # 3. The socket rule, on real listeners. Two of them bind to every interface for about 25 s
  # (the controls need real wildcard listeners); a firewall prompt may appear.
  echo "note: the socket controls open test listeners on 0.0.0.0 and :: for about 25 s"
  local tmp lpid lport raw kind
  tmp="$(mktemp -d "$OUT/controlsXXXX")"
  ( serve_listener 0.0.0.0 >"$tmp/wild4" ) &
  disown
  ( serve_listener 127.0.0.1 >"$tmp/loop4" ) &
  disown
  ( serve_listener :: >"$tmp/wild6" ) &
  disown
  local waited
  for waited in $(seq 1 50); do
    [ -s "$tmp/loop4" ] && [ -s "$tmp/wild4" ] && [ -s "$tmp/wild6" ] && break
    sleep 0.1
  done
  for kind in loop4 wild4 wild6; do
    [ -s "$tmp/$kind" ] || fail "the test listener $kind did not start within 5 s"
    lpid="$(cut -d' ' -f1 "$tmp/$kind")"
    lport="$(cut -d' ' -f2 "$tmp/$kind")"
    raw="$(lsof_raw "$lpid")"
    outp="$(echo "$raw" | socket_violations)"
    rc=0
    [ -z "$outp" ] || rc=1
    # lsof must have listed the listener itself: an empty reading is not a clean one.
    grep -q "^n.*:$lport" <<<"$raw" || fail "lsof did not list the test listener $kind on port $lport (lsof: ${raw:-nothing}; stderr: $(cat "$OUT/lsof.err"))"
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
  control_record "window rule: empty title is BLOCKED (exit 3), not a pass" refuse:3 "$rc" "$outp"

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

  # 5b. The text-only rule: an image under the evidence folder is refused.
  mkdir -p "$tmp/textonly" "$tmp/withimage"
  printf 'RESULT: text\n' >"$tmp/textonly/log.txt"
  printf 'RESULT: text\n' >"$tmp/withimage/log.txt"
  python3 -I -c 'import sys, zlib, struct
def chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"\x00\x00\x00\x00")) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)' "$tmp/withimage/shot.png"
  rc=0
  outp="$(assert_text_only "$tmp/textonly" 2>&1 | sed "s#$tmp/##g")" || rc=$?
  control_record "text-only rule: a folder with one text log" accept "$rc" "${outp:-text only}"
  rc=0
  outp="$(assert_text_only "$tmp/withimage" 2>&1 | sed "s#$tmp/##g")" || rc=$?
  control_record "text-only rule: a folder with a PNG image" refuse "$rc" "$outp"

  # 6. The blocked path: a missing identity is BLOCKED, never a pass.
  rc=0
  outp="$(APP_CHECK_IDENTITY="No Such Identity" APP_CHECK_EVIDENCE="$tmp/evidence" bash "$SELF" prereq 2>&1)" || rc=$?
  # Exit 3 alone is not enough: any other missing tool or a locked screen also gives 3. The cause must be the identity.
  grep -q "No Such Identity" <<<"$outp" || rc=1
  control_record "prerequisites with a signing identity that does not exist (BLOCKED is exit 3, caused by the identity)" refuse:3 "$rc" "$outp"

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
  assert_text_only "$EVID" || fail "an image or binary file is under the repository evidence folder (screenshots belong in $SHOTS)"

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
    echo "The raw files in this folder are the evidence; this page only quotes them. \`SHA256SUMS\` lists every other file and the hash of each screenshot with its local path."
    echo "Screenshots are not in the repository: they are stored in \`$SHOTS\`."
    echo "Verify the text files: \`cd docs/m0/M0b-01/evidence && grep -v ' /' SHA256SUMS | shasum -a 256 -c\`. The lines that start with an absolute path are the screenshots: they verify only on the machine that holds them."
    echo
    echo "## Quoted result lines"
    echo
    for f in 00-bundle.txt 01-build-flags.txt 02-window-poll.log $(for i in $(seq 1 "$IDLE_LAUNCHES"); do echo "03-sockets-launch$i.log"; done) 03-interrupted-launches.log $(for i in $(seq 1 "$IDLE_LAUNCHES"); do echo "04-rss-launch$i.txt"; done) 04-rss-all-launches.txt 05-negative-controls.txt 06-checksum-control.txt 07-telex-typing.log; do
      echo "- \`$f\`:"
      if [ -f "$EVID/$f" ]; then
        grep -E '^(RESULT|exit: 0|FLAGS)' "$EVID/$f" | sed 's/^/  > /' || true
      else
        if [ "$f" = "07-telex-typing.log" ]; then
          echo "  > NOT RUN (the typing check is opt-in: scripts/check.sh app --typing)"
        else
          echo "  > none (no such file in this run)"
        fi
      fi
    done
    echo
    echo "## Screenshots (stored locally, not in git)"
    echo
    if [ -s "$OUT/shots.list" ]; then
      while IFS= read -r f; do echo "- \`$(shasum -a 256 "$f")\`"; done <"$OUT/shots.list"
    else
      echo "- none in this run"
    fi
    echo
    echo "## Negative controls (05-negative-controls.txt)"
    echo
    grep -E '^CONTROL|verdict:' "$EVID/05-negative-controls.txt" | paste - - | sed 's/\t  verdict:/  ->/' | sed 's/^/- /'
    echo
    echo "## Limits"
    echo
    sed 's/^# //; s/^/- /' "$EVID/03-limits.txt" | tail -n +2
    echo "- The screenshots show the window at two moments (the first poll that found it, and 5 s later); they are not a recording, and they are kept outside git."
    echo "- Rerun \`scripts/check.sh app\` to compare: the pass lines must repeat, the memory numbers vary (read the median and the spread)."
  } >"$sum"

  (cd "$EVID" && find . -type f ! -name SHA256SUMS | sort | sed 's#^\./##' | while IFS= read -r f; do shasum -a 256 "$f"; done >SHA256SUMS)
  # The screenshots are outside the repository: their hashes carry their local path.
  if [ -s "$OUT/shots.list" ]; then
    while IFS= read -r f; do shasum -a 256 "$f" >>"$EVID/SHA256SUMS"; done <"$OUT/shots.list"
  fi
  assert_text_only "$EVID" || fail "an image or binary file is under the repository evidence folder"
  (cd "$EVID" && shasum -a 256 -c SHA256SUMS >"$OUT/sums-check.txt" 2>&1) || fail "SHA256SUMS does not verify"
  echo "SUMMARY.md written; SHA256SUMS covers $(wc -l <"$EVID/SHA256SUMS" | tr -d ' ') files (screenshots by local path) and verifies"
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
    telex) step_telex ;;
    controls) step_controls ;;
    evidence) step_evidence ;;
    *)
      echo "usage: scripts/app-check.sh prereq|build-proof|bundle|window|idle|telex|controls|evidence|override-guard" >&2
      exit 64
      ;;
  esac
}

# Sourced by scripts/app-check.test.sh: define the functions and stop.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
