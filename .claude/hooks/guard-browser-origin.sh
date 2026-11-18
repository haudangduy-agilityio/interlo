#!/usr/bin/env bash
#
# PreToolUse hook: deny browser navigation to any address that is not on this machine.
#
# docs/guardrails.md rule 1 says nothing leaves the Mac during a meeting. This hook keeps
# the browser tools used for UI checks on the same rule: they may open the local dev server
# and nothing else.
#
# Contract (https://code.claude.com/docs/en/hooks):
#   - Hook input arrives as JSON on stdin.
#   - Printing a `permissionDecision` of "deny" blocks the call. Exit code stays 0.
#
# Registered in .claude/settings.json under PreToolUse. The matcher names whole browser
# servers (Playwright, Claude in Chrome, the built-in browser), not single tools, so a new
# tool on those servers is checked without a settings change. A call with no address in it
# passes.
#
# What it checks: every value stored under a key named url, uri or href, at any depth.
# browser_batch hides its navigate calls under actions[].input.url, and browser_tabs opens
# a new tab with a url. A value that is not a string, or that holds a space, a control
# character or a backslash, is denied, because a browser can read it as a different host.
#
# Allowed addresses, matched exactly: localhost, 127.0.0.1 or [::1] with an explicit port
# from 1024 to 65535 (the Vite dev server is 5173, a preview server 4173, and the end to end
# runner picks its own). A port is required, so a bare `localhost` and any system port are
# denied. Alternate spellings such as 127.1, localhost. and *.localhost are denied on
# purpose. Allowed schemes: http and https, or none. `back`, `forward` and `about:blank` pass.
#
# Not covered, so this hook is not a sandbox on its own:
#   - A page that links or redirects to another site, and page code run through
#     browser_evaluate or javascript_tool.
# The permission deny rules in settings.json cover the tools that run page code.
#
# Fails closed: input that cannot be parsed is denied, and so is a missing jq. Empty
# input is allowed because there is nothing to check.
#
# Needs jq.

set -uo pipefail

INPUT=$(cat)

if [ -z "$INPUT" ]; then
  exit 0
fi

deny() {
  # Fixed text only, so the JSON stays valid whatever the address contains.
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$1"
  exit 0
}

NOT_LOCAL="Browser navigation is limited to localhost, 127.0.0.1 and [::1] with an explicit port from 1024 to 65535. Blocked by the guard-browser-origin hook. Start the app locally and open its localhost address."
UNREADABLE="The guard-browser-origin hook could not read an address in the tool input, so it blocked the call."

if ! command -v jq >/dev/null 2>&1; then
  deny "The guard-browser-origin hook needs jq to read the tool input, so it blocked the call. Install jq, or remove the hook from .claude/settings.json."
fi

# One lowercase address per line. A newline inside an address is a control character, so
# it is rejected here and never splits an address into two.
if ! URLS=$(printf '%s' "$INPUT" | jq -r '
  [.tool_input | .. | objects | to_entries[] | select(.key | test("^(url|uri|href)$"; "i")) | .value]
  | map(
      if type != "string" then error("not a string")
      elif explode | any(. <= 32 or . == 127 or . == 92) then error("control character, space or backslash")
      else ascii_downcase
      end
    )
  | .[]' 2>/dev/null); then
  deny "$UNREADABLE"
fi

while IFS= read -r URL; do
  if [ -z "$URL" ]; then
    continue
  fi

  case "$URL" in
  back | forward | about:blank) continue ;;
  esac

  # The authority ends at the first / ? or #. Only an http or https address has user info,
  # where in a@b the host is b. With no scheme an @ means nothing: javascript:x@localhost
  # is code, not a host, so nothing is stripped and the whole authority must be local.
  if [[ "$URL" == *"://"* ]]; then
    case "${URL%%://*}" in
    http | https) ;;
    *) deny "$NOT_LOCAL" ;;
    esac
    REST="${URL#*://}"
    AUTHORITY="${REST%%[/?#]*}"
    AUTHORITY="${AUTHORITY##*@}"
  else
    AUTHORITY="${URL%%[/?#]*}"
  fi

  if ! [[ "$AUTHORITY" =~ ^(localhost|127\.0\.0\.1|\[::1\]):([1-9][0-9]{3,4})$ ]]; then
    deny "$NOT_LOCAL"
  fi
  # The pattern admits 10000 to 99999, so the upper bound is checked as a number.
  # 10# keeps bash from reading the digits as octal.
  if [ "$((10#${BASH_REMATCH[2]}))" -lt 1024 ] || [ "$((10#${BASH_REMATCH[2]}))" -gt 65535 ]; then
    deny "$NOT_LOCAL"
  fi
done <<<"$URLS"

exit 0
