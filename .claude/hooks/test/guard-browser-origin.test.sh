#!/usr/bin/env bash
#
# Tests for guard-browser-origin.sh. Each case feeds the hook one synthetic tool call on
# stdin and checks the decision it prints. No browser is started.
#
# Every case checks three things together: the decision on stdout (empty means allow), the
# exit code (always 0), and that stderr is empty. Claude Code reads the decision from
# stdout, so a decision on stderr must fail a case.
#
# Usage: .claude/hooks/test/guard-browser-origin.test.sh
# Needs jq.

set -uo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
HOOK="$REPO_ROOT/.claude/hooks/guard-browser-origin.sh"

command -v jq >/dev/null 2>&1 || {
  echo "jq is required to build the test input" >&2
  exit 2
}
[ -x "$HOOK" ] || {
  echo "hook is missing or not executable: $HOOK" >&2
  exit 2
}

PASS=0
FAIL=0

# Run the hook with raw stdin $1. Sets OUT (stdout), ERR (stderr) and CODE.
run_raw() {
  local err_file
  err_file=$(mktemp)
  OUT=$(printf '%s' "$1" | "$HOOK" 2>"$err_file")
  CODE=$?
  ERR=$(cat "$err_file")
  rm -f "$err_file"
}

# Run the hook for a navigate call to address $1.
run_url() {
  run_raw "$(jq -n --arg url "$1" '{tool_name:"mcp__playwright__browser_navigate",tool_input:{url:$url}}')"
}

expect() {
  if [ "$2" = "$3" ]; then
    PASS=$((PASS + 1))
    echo "ok   $1"
  else
    FAIL=$((FAIL + 1))
    echo "FAIL $1"
    echo "  expected: $2"
    echo "  actual:   $(printf '%s' "$3" | head -c 300)"
  fi
}

# The decision printed on stdout: allow when it is empty, deny when it is a deny decision.
decision() {
  if [ -z "$OUT" ]; then
    echo allow
  elif [ "$(printf '%s' "$OUT" | jq -r '.hookSpecificOutput.permissionDecision' 2>/dev/null)" = deny ]; then
    echo deny
  else
    echo "other: $OUT"
  fi
}

# Check the last run against decision $2, exit code 0 and an empty stderr.
check() {
  expect "$1" "$2|0|" "$(decision)|$CODE|$ERR"
}

allow() {
  run_url "$1"
  check "allow $(printf '%q' "$1")" allow
}

deny() {
  run_url "$1"
  check "deny  $(printf '%q' "$1")" deny
}

allow_json() {
  run_raw "$2"
  check "allow $1" allow
}

deny_json() {
  run_raw "$2"
  check "deny  $1" deny
}

# The local apps
allow 'http://localhost:4173'
allow 'http://localhost:4173/path?x=1#top'
allow 'https://localhost:9323'
allow 'http://127.0.0.1:5173/'
allow 'http://[::1]:9323/health'
allow 'HTTP://LOCALHOST:5173'
allow 'localhost:4173'
allow 'http://localhost:5173?x=1'
allow 'http://localhost:5173#top'

# User info does not change the host. The host is the part after the last @.
allow 'http://someone@localhost:5173/'
allow 'http://example.com@localhost:5173/'
allow 'http://user:pw@127.0.0.1:5173'
allow 'http://a@b@localhost:5173/'

# Calls that open no address
allow 'back'
allow 'forward'
allow 'about:blank'

# Other sites
deny 'https://example.com'
deny 'HTTP://EXAMPLE.COM'
deny 'example.com'
deny 'http://localhost.example.com'
deny 'http://notlocalhost'
deny 'http://127.0.0.1.example.com'
deny 'http://10.0.0.5:5173'
deny 'https://example.com/?next=http://localhost'

# The authority ends at /, ? or #. Each terminator is tested alone.
deny 'https://example.com/#@localhost'
deny 'http://example.com?@localhost'
deny 'http://example.com#@localhost'

# A port is required and it must be from 1024 to 65535
deny 'http://localhost'
deny 'localhost'
deny 'http://127.0.0.1:22'
deny 'http://localhost:80'
deny 'http://localhost:443'
deny 'http://localhost:1023'
deny 'http://localhost:0'
deny 'http://localhost:65536'
deny 'http://localhost:99999'
deny 'http://localhost:100000'
deny 'http://localhost:07000'
allow 'http://localhost:1024'
allow 'http://localhost:65535'
allow 'https://localhost:8443'
allow 'http://localhost:3100'

# Spellings of the local host that are denied on purpose
deny 'http://app.localhost:5173'
deny 'http://evil.localhost'
deny 'http://localhost.'
deny 'http://127.1'
deny 'http://2130706433'
deny 'http://0x7f.1'
deny 'http://0.0.0.0:5173'
deny 'http://[::ffff:127.0.0.1]'
deny 'http://localhost:'
deny 'http://localhost:abc'
deny 'http://[::1'

# Tricks that make a browser read a different host
deny 'http://localhost@example.com/'
deny 'http://localhost:5173@example.com/'
deny 'http://user:pa@ss@example.com/'
deny 'http://example.com @localhost/'
deny $'http://example.com\x7f@localhost/'
deny 'http://example.com\@localhost/'
deny 'http://localhost%2eexample.com'
deny 'x y.localhost'
deny 'localhost!'
deny 'http:///path'
deny '//example.com'
deny $'http://localhost:5173\thttp://example.com'
deny $'http://localhost:5173\nhttp://example.com'
deny $'http://localhost:5173\r'

# The same tricks again, each with an allowed port. Without a port they would be denied
# for that reason alone, and a broken check for the trick itself would go unseen.
deny 'http://localhost.example.com:5173'
deny 'http://localhostevil:5173'
deny 'http://example.com\@localhost:5173/'
deny 'http://example.com @localhost:5173/'
deny $'http://example.com\x7f@localhost:5173/'
deny $'http://example.com\t@localhost:5173/'
deny 'file://localhost:5173/etc/passwd'
deny 'ftp://localhost:5173/'

# Schemes that are not http or https
deny 'file:///etc/passwd'
deny 'file://localhost/etc/passwd'
deny 'javascript:alert(1)'
deny 'data:text/html,x'
deny 'ftp://localhost/'
deny 'ws://localhost:5173'
deny 'blob:http://localhost/x'
deny 'view-source:http://localhost:5173'

# Without a scheme there is no user info, so an @ cannot hide what comes before it.
# javascript: decodes %2f when it runs, so the last case navigates away.
deny 'javascript:alert(1)@localhost'
deny 'data:,x@localhost'
deny 'mailto:a@localhost'
deny 'vbscript:x@localhost'
deny 'javascript:alert(1)@localhost:5173'
deny 'data:,x@localhost:5173'
deny 'mailto:a@localhost:5173'
deny "javascript:location='http:%2f%2fexample.com'%2f%2f@localhost:5173"
deny "javascript:location='http:%2f%2fexample.com'%2f%2f@localhost"
allow 'localhost:5173/a@b'

# Words that only look like the calls that open no address
deny 'back.example.com'
deny 'forward.example.com'
deny 'about:srcdoc'

# The address can sit anywhere in the input and under any key named url, uri or href.
# browser_batch nests its navigate calls under actions[].input.
BATCH_LOCAL='{"tool_input":{"actions":[{"name":"navigate","input":{"url":"http://localhost:4173"}}]}}'
BATCH_FAR='{"tool_input":{"actions":[{"name":"navigate","input":{"url":"https://example.com"}}]}}'
BATCH_MIXED='{"tool_input":{"actions":[{"name":"navigate","input":{"url":"http://localhost:4173"}},{"name":"navigate","input":{"url":"https://example.com"}}]}}'
allow_json "a browser_batch that opens only a local address" "$BATCH_LOCAL"
deny_json "a browser_batch that opens another site" "$BATCH_FAR"
deny_json "a browser_batch where only the second call opens another site" "$BATCH_MIXED"
deny_json "browser_tabs opening a new tab on another site" '{"tool_input":{"action":"new","url":"https://example.com"}}'
allow_json "browser_tabs listing tabs" '{"tool_input":{"action":"list"}}'
deny_json "an address under the key uri" '{"tool_input":{"uri":"https://example.com"}}'
deny_json "an address under the key href" '{"tool_input":{"href":"https://example.com"}}'
deny_json "an address under the key URL in capitals" '{"tool_input":{"URL":"https://example.com"}}'
allow_json "an element selector under the key target is not an address" '{"tool_input":{"target":"e14","element":"Sign in"}}'

# A call with no address passes. Input that cannot be read does not.
allow_json "a call with no url (preview_start by name)" '{"tool_name":"mcp__Claude_Browser__preview_start","tool_input":{"name":"interlo-ui"}}'
allow_json "a tool_input that is null" '{"tool_input":null}'
allow_json "empty input (nothing to check)" ''
deny_json "input that is not JSON" 'not json'
deny_json "a url that is a number" '{"tool_input":{"url":42}}'
deny_json "a url that is a list" '{"tool_input":{"url":["http://localhost:5173"]}}'
deny_json "a url that is an object" '{"tool_input":{"url":{"a":"localhost"}}}'
deny_json "a url that is null" '{"tool_input":{"url":null}}'

# The decision has the shape Claude Code reads
run_url 'https://example.com'
expect "deny output has one top-level key, the PreToolUse event, a deny and a reason" \
  'hookSpecificOutput|PreToolUse|deny|true|3' \
  "$(printf '%s' "$OUT" | jq -r '[(keys | join(",")), .hookSpecificOutput.hookEventName, .hookSpecificOutput.permissionDecision, (.hookSpecificOutput.permissionDecisionReason | length > 0), (.hookSpecificOutput | keys | length)] | join("|")')"

# Without jq the hook cannot read the input, so it denies. A directory that holds only cat
# on PATH hides jq and keeps the shell able to read stdin.
NO_JQ_DIR=$(mktemp -d)
ln -s "$(command -v cat)" "$NO_JQ_DIR/cat"
OUT=$(printf '%s' '{"tool_input":{"url":"http://localhost:5173"}}' | PATH="$NO_JQ_DIR" /bin/bash "$HOOK" 2>"$NO_JQ_DIR/err")
CODE=$?
ERR=$(cat "$NO_JQ_DIR/err")
rm -rf "$NO_JQ_DIR"
check "deny  every call when jq is missing" deny
expect "the reason for a missing jq says the hook needs jq" true \
  "$(printf '%s' "$OUT" | jq -r '.hookSpecificOutput.permissionDecisionReason | contains("needs jq")')"

echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
