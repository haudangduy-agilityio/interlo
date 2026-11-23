#!/usr/bin/env bash
# Format, lint and unit tests of the spike crate. The root gate (scripts/check.sh) does not
# build bench/ crates (D-190), so this script is their gate.
# Usage: bench/M0b-05/check.sh
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
echo "ALL STEPS PASSED (bench/M0b-05)"
