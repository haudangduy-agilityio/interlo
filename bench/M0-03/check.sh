#!/usr/bin/env bash
# Format, lint, tests, names and language, and no media of the harness crate. The root gate
# (scripts/check.sh) does not build bench/ crates (D-190), so this script is their gate.
# Usage: bench/M0-03/check.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(git -C "$HERE" rev-parse --show-toplevel)"
cd "$HERE"

command -v rg > /dev/null || { echo "NOT CONFIGURED: rg is not installed" >&2; exit 2; }

cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked

# Names (guardrail 3) and language (guardrail 4) in the crate and its note. This file holds the
# name list, so the name scan skips it; the language scan reads it.
NAMES='(whisper|silero|piper|kokoro|zoom|slack|chrome|safari|firefox|github|gitlab|google|microsoft|onnx|ort)'
SCOPE=("$HERE" "$ROOT/docs/m0/M0-03.md")
[ -f "$ROOT/docs/m0/M0-03.md" ] || { echo "missing $ROOT/docs/m0/M0-03.md" >&2; exit 1; }

# scan <what> <rg args...>: rg exits 0 on a match, 1 on no match and 2 on an error. Only 1 passes.
scan() {
  local what="$1" rc=0
  shift
  rg "$@" || rc=$?
  case "$rc" in
    0) echo "$what is in the files above" >&2; exit 1 ;;
    1) ;;
    *) echo "rg failed (exit $rc) while checking $what" >&2; exit 1 ;;
  esac
}
scan "a name from the forbidden list" -n -i -w "$NAMES" "${SCOPE[@]}" --glob '!Cargo.lock' --glob '!target' --glob '!check.sh'
scan "non-ASCII text" -n '[^\x00-\x7F]' "${SCOPE[@]}" --glob '!target'

# No media or model file is tracked or staged in the crate or its note.
MEDIA='\.(wav|png|jpe?g|gif|mp3|m4a|flac|ogg|pcm|raw|onnx|bin)$'
TRACKED="$(git -C "$ROOT" ls-files bench/M0-03 docs/m0/M0-03.md)" || exit 1
UNTRACKED="$(git -C "$ROOT" ls-files -o --exclude-standard bench/M0-03 docs/m0/M0-03.md)" || exit 1
FILES="$TRACKED"$'\n'"$UNTRACKED"
[ -n "$FILES" ] || { echo "git listed no files for bench/M0-03" >&2; exit 1; }
printf '%s\n' "$FILES" | scan "a media or model file" -i "$MEDIA"
echo "ALL STEPS PASSED (bench/M0-03)"
