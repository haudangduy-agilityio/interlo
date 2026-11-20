#!/usr/bin/env bash
# Self-test of the pure parts of scripts/app-check.sh: the socket rule and the
# memory statistics. It needs no display and no permission, so it runs in the
# gate (scripts/check.sh quick). Each rule has a case that must pass and a case
# that must be refused.
set -euo pipefail

# shellcheck source=scripts/app-check.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/app-check.sh"

FAILURES=0

expect_empty() { # name text
  if [ -n "$2" ]; then
    echo "FAIL: $1: expected no violation, got: $2"
    FAILURES=$((FAILURES + 1))
  else
    echo "ok: $1"
  fi
}

expect_flagged() { # name text needle
  if echo "$2" | grep -q -- "$3"; then
    echo "ok: $1"
  else
    echo "FAIL: $1: expected a violation containing \"$3\", got: \"$2\""
    FAILURES=$((FAILURES + 1))
  fi
}

expect_equal() { # name expected actual
  if [ "$2" = "$3" ]; then
    echo "ok: $1"
  else
    echo "FAIL: $1: expected \"$2\", got \"$3\""
    FAILURES=$((FAILURES + 1))
  fi
}

lsof_out() { printf '%s\n' "$@"; }

expect_empty "no socket at all" "$(printf '' | socket_violations)"
expect_empty "loopback connection" "$(lsof_out p100 PTCP n127.0.0.1:50000-\>127.0.0.1:8080 | socket_violations)"
expect_empty "loopback listener with a state" "$(lsof_out p100 PTCP 'n127.0.0.1:1234 (LISTEN)' | socket_violations)"
expect_empty "IPv6 loopback" "$(lsof_out p100 PTCP 'n[::1]:50000->[::1]:8080' | socket_violations)"
expect_empty "localhost name" "$(lsof_out p100 PTCP n127.0.0.1:50001-\>localhost:8080 | socket_violations)"

expect_flagged "wildcard listener" "$(lsof_out p100 PTCP 'n*:8080 (LISTEN)' | socket_violations)" '\*:8080'
expect_flagged "wildcard UDP socket" "$(lsof_out p100 PUDP 'n*:*' | socket_violations)" '\*:\*'
expect_flagged "remote address" "$(lsof_out p100 PTCP n127.0.0.1:50000-\>93.184.216.34:443 | socket_violations)" '93.184.216.34'
expect_flagged "remote address on the local side" "$(lsof_out p100 PTCP n10.0.0.5:50000-\>127.0.0.1:8080 | socket_violations)" '10.0.0.5'
expect_flagged "IPv6 remote address" "$(lsof_out p100 PTCP 'n[::1]:50000->[2001:db8::1]:443' | socket_violations)" '2001:db8::1'
expect_flagged "the pid is named" "$(lsof_out p4242 PTCP 'n*:80 (LISTEN)' | socket_violations)" 'pid 4242'

expect_equal "median of five" "5" "$(printf '9\n1\n5\n7\n3\n' | median)"
expect_equal "median of an even count is the lower middle" "2" "$(printf '4\n1\n3\n2\n' | median)"
expect_equal "minimum" "1" "$(printf '9\n1\n5\n' | min_of)"
expect_equal "maximum" "9" "$(printf '9\n1\n5\n' | max_of)"
if printf '' | median >/dev/null 2>&1; then
  echo "FAIL: median of nothing must fail"
  FAILURES=$((FAILURES + 1))
else
  echo "ok: median of nothing fails"
fi

expect_status() { # name expected_status actual_status
  expect_equal "$1" "$2" "$3"
}

status_of() { local rc=0; "$@" >/dev/null 2>&1 || rc=$?; echo "$rc"; }

REAL_BIN="rustc --crate-name interlo_desktop --crate-type bin -C opt-level=3 -C lto=thin -C codegen-units=1 -C strip=symbols"
expect_status "rustc line of the agreed profile" 0 "$(status_of assert_rustc_line "$REAL_BIN" bin)"
expect_status "rustc line with opt-level=1" 1 "$(status_of assert_rustc_line "${REAL_BIN//opt-level=3/opt-level=1}" bin)"
expect_status "rustc line with codegen-units=16" 1 "$(status_of assert_rustc_line "${REAL_BIN//codegen-units=1/codegen-units=16}" bin)"
expect_status "bin line without lto=thin" 1 "$(status_of assert_rustc_line "${REAL_BIN//-C lto=thin/}" bin)"
expect_status "rustc line without a strip flag" 1 "$(status_of assert_rustc_line "${REAL_BIN//-C strip=symbols/}" bin)"
expect_status "rustc line with panic=abort" 1 "$(status_of assert_rustc_line "$REAL_BIN -C panic=abort" bin)"
expect_status "lib line needs no lto=thin" 0 "$(status_of assert_rustc_line "rustc --crate-type lib -C opt-level=3 -C linker-plugin-lto -C codegen-units=1 -C strip=symbols" lib)"

expect_status "window of 1200 by 780 named Interlo" 0 "$(status_of assert_window Interlo 1200 780 1324)"
expect_status "window of 1000 by 700" 1 "$(status_of assert_window Interlo 1000 700 1324)"
expect_status "window named Other" 1 "$(status_of assert_window Other 1200 780 1324)"
expect_status "window found after 61 s" 1 "$(status_of assert_window Interlo 1200 780 61000)"
expect_status "window with an empty title is BLOCKED" 3 "$(status_of assert_window "" 1200 780 1324)"

RSS_DIR="$(mktemp -d "${TMPDIR:-/tmp}/app-check-testXXXXXX")"
printf 'rss_kb=1\nrss_kb=2\nrss_kb=3\nrss_kb=4\nrss_kb=5\n' >"$RSS_DIR/ok.txt"
printf 'rss_kb=1\nrss_kb=2\nrss_kb=3\n' >"$RSS_DIR/short.txt"
expect_status "five memory readings" 0 "$(status_of assert_rss_file "$RSS_DIR/ok.txt")"
expect_status "three memory readings" 1 "$(status_of assert_rss_file "$RSS_DIR/short.txt")"
expect_equal "memory readings are read back" "1 2 3 4 5" "$(rss_values "$RSS_DIR/ok.txt" | tr '\n' ' ' | sed 's/ $//')"
rm -f "$RSS_DIR/ok.txt" "$RSS_DIR/short.txt"
rmdir "$RSS_DIR"

if [ "$FAILURES" -ne 0 ]; then
  echo "$FAILURES failed"
  exit 1
fi
echo "all passed"
