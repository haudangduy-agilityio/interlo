#!/usr/bin/env bash
# Build the spike and wrap it in a signed local .app (bundle id app.interlo.bench.m0b05).
# A bundle is needed so the audio-capture permission attaches to the spike, not to a terminal.
# Usage: bench/M0b-05/bundle.sh   (prints the path of the bundle on the last line)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IDENTITY="${M0B05_IDENTITY:-Interlo Local Signing}"
cd "$HERE"
cargo build --release --locked
APP="$HERE/target/bundle/m0b05.app"
mkdir -p "$APP/Contents/MacOS"
cp target/release/m0b05 "$APP/Contents/MacOS/m0b05"
cp Info.plist "$APP/Contents/Info.plist"
codesign --force --sign "$IDENTITY" --identifier app.interlo.bench.m0b05 "$APP"
codesign --verify --deep --strict "$APP"
echo "$APP"
