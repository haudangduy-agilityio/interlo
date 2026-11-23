#!/usr/bin/env bash
# Fail when an image, audio or other binary media file is tracked or staged in the repository.
# Screenshots and recordings live outside git (owner rule, 2026-11-23); only text evidence is committed.
# The repository check covers bench/ and docs/m0/ (the app icons elsewhere are product assets).
# Usage: bench/M0b-05/no-media.sh            check the repository (tracked and staged files)
#        printf 'a.png\n' | bench/M0b-05/no-media.sh -   check a list of paths from stdin
set -euo pipefail
PATTERN='\.(png|jpe?g|gif|webp|bmp|tiff?|heic|svg|ico|pdf|mp4|mov|mkv|webm|wav|aiff?|caf|m4a|mp3|flac|ogg|opus|pcm|raw)$'
if [ "${1:-}" = "-" ]; then
  LIST="$(cat)"
else
  ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
  LIST="$( (git -C "$ROOT" ls-files; git -C "$ROOT" diff --cached --name-only) | sort -u | grep -E '^(bench/|docs/m0/)' || true)"
fi
HITS="$(printf '%s\n' "$LIST" | grep -i -E "$PATTERN" || true)"
if [ -n "$HITS" ]; then
  echo "media files in the repository:" >&2
  printf '%s\n' "$HITS" >&2
  exit 1
fi
echo "no media files"
