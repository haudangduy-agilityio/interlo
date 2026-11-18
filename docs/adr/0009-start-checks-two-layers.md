# 0009. Start checks run in two layers

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-44

## Context

One start gate with five required items mixed two kinds of check. Some items
need installed things (permissions, devices, models) and change rarely. Others
depend on the meeting (headphones, the meeting app's microphone, the chosen
meeting app) and must be confirmed every time. The old wording also required
that only one virtual device was installed, which blocked the process tap route.

## Decision

Layer 1, readiness (`check_setup`, `canStart`), is one rule for every mode:
`canStart` = microphone ok AND capture ok AND virtual microphone ok AND the four
models ok (speech recognition, translation, English voice, Vietnamese voice).
Capture is ok when the process tap for the chosen meeting app works or the
2-channel fallback device is installed. Headphones never block readiness.
`missingGroups` holds display names in this order: "Microphone", "Meeting audio",
"Virtual microphone", "Models".

Layer 2, per-meeting checks, are confirmed by the owner at every Start:
`start_meeting` takes `checks: { headphonesConfirmed, meetingMicConfirmed }`
and a `meetingAppId`. The checks are "headphones" (FR-72), "meetingMic"
(FR-69), "meetingApp" (FR-68) and "routing" (the capture route and the English
voice output must not share one virtual device). A failed check returns
`START_CHECK_FAILED` with `details` listing the failed check ids.

## Consequences

- The Setup screen and the Start gate agree on one rule, and the per-meeting
  items are asked at the moment they matter.
- `start_meeting` refuses in this order: INVALID_INPUT, MEETING_RUNNING,
  SETUP_INCOMPLETE, START_CHECK_FAILED. The contract is in `docs/ipc-contract.md`.
- The error catalogue gets CHK-01 "Confirm before starting". MIC-01 and DEV-01
  stay.
- Tests cover each failed check and the refusal order.
