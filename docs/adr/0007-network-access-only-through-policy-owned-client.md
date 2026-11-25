# 0007. Network access only through a policy-owned client

- Status: Accepted (part 1); part 2 Proposed
- Date: 2026-11-17
- Decision ID: D-48

## Context

The policy gate checks a provider description before an outbound call.
`provider.remote` is declared by the caller, so a caller can declare itself
local and skip the check. Any crate that can open a socket can bypass the gate.
Guardrail 1 says every outbound call goes through the gate.

## Decision

Part 1 (accepted). Only the policy can construct a network client type. Code
that needs the network receives that client from the policy and has no other
way to open a connection. A CI check fails when any crate outside the policy
opens a socket.

Part 2 (proposed, deferred to M4). Remove the outbound-network entitlement from
the main app and run model downloads, the driver installer download and the
update check (ADR 0015, ADR 0016) in a helper process that holds it, with the
App Sandbox. Before this part is accepted, verify that the sandbox still
allows the Accessibility API that quick actions need (E2).

## Consequences

- A call that skips the gate does not compile, and the CI check catches a crate
  that opens a socket by another route. The check is not written yet.
- Tests use a fake client built by the policy test helpers.
- Until part 2 is accepted, the operating system does not block the main app
  from the network. Only the type and the CI check enforce the gate.
- Part 2 would add a second binary to build and sign (ADR 0022), and
  an IPC path between the app and the helper.
- Status (M1-12): `GuardedClient` in `crates/meeting-core/src/egress.rs` is the policy-owned client. It wraps a `Transport` trait whose `get` needs a `Permit` that only the guard can build, and it refuses and logs a request before the transport is reached (https url, no active meeting session, then the policy rule of the purpose). No real transport exists yet: M1-04.4 and M4-19 write one and move their callers onto the guard. The CI check for a crate that opens a socket by another route is still not written (X-16 part b). `Provider` can no longer
  be built with a free `remote` field: use `Provider::local` or `Provider::remote`.
  A first check exists in `apps/desktop/src-tauri/tests/network_boundary.rs`: it
  fails when a workspace crate declares an HTTP or socket crate, or when source
  opens a `TcpStream` or `UdpSocket`. It does not see transitive dependencies or
  runtime behaviour, and it is not a CI step of its own (it runs inside
  `cargo test`). Part 2 is not started.
