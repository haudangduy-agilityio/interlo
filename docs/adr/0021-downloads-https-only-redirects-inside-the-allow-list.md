# 0021. Downloads use https only and follow a redirect only inside the pinned allow list

- Status: accepted
- Date: 2026-11-19
- Decision ID: D-175 (the split of X-16 is D-174)

## Context

The model downloader exists from day 3 (M1-04). D-136 item (c) said a download
never follows a redirect to another host. Model hosts often serve the file from
a separate content host. A rule that refuses every redirect would break the
download of a pinned model. A server on an allowed host can send the client
anywhere. A rule that follows redirects without a check would open a hole in
the policy gate (ADR 0007).

## Decision

A download of a model, a driver installer, an update manifest or an update
package follows a redirect under three limits. The host is on the pinned allow
list. The redirect uses https. The hop count stays within a fixed limit that
the code sets and a test checks. The policy checks every hop. The code checks
the hash of the final file before use. A redirect
to any other host fails the download and logs NET-01. This replaces "never
follows a redirect to another host" in D-136 item (c) and in FR-75.

## Consequences

- The hash is the final guard: even a redirect inside the list cannot deliver a
  file that is not the pinned one.
- `check_redirect` in `crates/model-store` already implements the allow-list
  and https rule for one hop. The downloader calls it for every hop and stops
  at the hop limit.
- X-16 is split (D-174). Part (a), the http and redirect tests, is part of the
  done checks of M1-04 and M1-42. Part (b) stays in X-16: a CI check that no
  crate outside the policy opens a socket, proven by a planted rogue crate.
  X-16 stays a v1.0 blocker (D-137).
- A host that needs a redirect to a second host must have both hosts on the
  allow list. That is a reviewed change to the manifest.
