#!/usr/bin/env bash
# Format, lint and unit tests of the spike crate. The root gate (scripts/check.sh) does not
# build bench/ crates (D-190), so this script is their gate.
# Usage: bench/M0b-05/check.sh
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
../M0b-05/no-media.sh
# negative control: the check must fail on a list that holds an image
if printf 'docs/shot.PNG\n' | ../M0b-05/no-media.sh - > /dev/null 2>&1; then
  echo "no-media.sh did not fail on an image path" >&2
  exit 1
fi
echo "ALL STEPS PASSED (bench/M0b-05)"
