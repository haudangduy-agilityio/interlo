#!/usr/bin/env bash
# Build the spike and wrap it in a local .app (bundle id app.interlo.bench.m0b06), so the window
# system treats it as an application and not as a child of the terminal.
# The signature is ad hoc by default (no Keychain prompt; the spike needs no permission that
# depends on the signing identity). Set M0B06_IDENTITY to sign with a named identity.
# Usage: bench/M0b-06/bundle.sh   (prints the path of the bundle on the last line)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IDENTITY="${M0B06_IDENTITY:--}"
cd "$HERE"
cargo build --locked
APP="$HERE/target/bundle/m0b06.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/debug/m0b06 "$APP/Contents/MacOS/m0b06"
cp Info.plist "$APP/Contents/Info.plist"
codesign --force --sign "$IDENTITY" --identifier app.interlo.bench.m0b06 "$APP"
codesign --verify --deep --strict "$APP"
echo "$APP"
