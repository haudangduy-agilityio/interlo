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
if rg -n -i -w "$NAMES" "${SCOPE[@]}" --glob '!Cargo.lock' --glob '!target' --glob '!check.sh'; then
  echo "a name from the forbidden list is in the files above" >&2
  exit 1
fi
if rg -n '[^\x00-\x7F]' "${SCOPE[@]}" --glob '!target'; then
  echo "non-ASCII text is in the files above" >&2
  exit 1
fi

# No media or model file is tracked or staged in the crate or its note.
MEDIA='\.(wav|png|jpe?g|gif|mp3|m4a|flac|ogg|pcm|raw|onnx|bin)$'
if (git -C "$ROOT" ls-files bench/M0-03 docs/m0/M0-03.md; git -C "$ROOT" ls-files -o --exclude-standard bench/M0-03 docs/m0/M0-03.md) | rg -i "$MEDIA"; then
  echo "a media or model file is in the repository" >&2
  exit 1
fi
echo "ALL STEPS PASSED (bench/M0-03)"
