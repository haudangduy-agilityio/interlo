# IPC contract

The Rust shell (`apps/desktop/src-tauri/src/ipc.rs`, `src/commands/`,
`src/events.rs`) and the UI (`apps/ui/src/ipc/types.ts`) implement this file.
Change all three in one change.

Rules: commands are snake_case; JSON fields are camelCase; every command returns
a value or rejects with `IpcError`. The UI calls commands through
`invoke` and `listen` of the `@tauri-apps/api` package (the global `window.__TAURI__` is off);
tests and the browser use the mock transport.

Sample payloads for every result, error, event and command argument are in
`fixtures/ipc/` (`args/` holds the arguments). The Rust and UI contract tests read them.

Webview rules: each command is granted to the main window by name
(`apps/desktop/src-tauri/capabilities/default.json`; the list is in `build.rs`).
A new command needs an entry in both files. The webview can show only the app's
own origin; any other navigation is denied.

The main window starts hidden (`"visible": false` in `tauri.conf.json`). The page shows it
after it has mounted (`apps/ui/src/ipc/showWindow.ts`), so the owner never sees the empty
native window colour. The capability grants `core:window:allow-show` for this and no other
window permission. If the page does not show the window, the shell does after 3 seconds
(`src/first_paint.rs`).

## Errors

```ts
type IpcError = {
  code:
    | "SETUP_INCOMPLETE" | "START_CHECK_FAILED" | "NOT_IMPLEMENTED"
    | "MEETING_RUNNING" | "NO_MEETING" | "INVALID_INPUT";
  message: string;
  details?: string[]; // only for START_CHECK_FAILED; absent from the JSON otherwise
};
```

`details` for START_CHECK_FAILED holds the ids of the failed per-meeting checks,
in the fixed order `"headphones"`, `"meetingMic"`, `"meetingApp"`, `"routing"`
(never empty, no duplicates). Every other code omits the field.

## Commands

| Command | Arguments | Returns | Errors |
|---|---|---|---|
| check_setup | none | `SetupStatus` | NOT_IMPLEMENTED (default build; the demo-data feature returns sample facts) |
| list_models | none | `ModelInfo[]` | NOT_IMPLEMENTED (default build; the demo-data feature returns sample models) |
| download_model | `{ id, licenceAccepted: boolean }` | nothing | INVALID_INPUT (unknown id, or licence not accepted), NOT_IMPLEMENTED (known id with licence accepted, skeleton) |
| cancel_model_download | `{ id }` | nothing | INVALID_INPUT (unknown id), NOT_IMPLEMENTED (known id, skeleton) |
| start_meeting | `{ mode: "off" \| "listen" \| "speak" \| "both", meetingAppId: string, checks: { headphonesConfirmed: boolean, meetingMicConfirmed: boolean } }` | `{ meetingId }` | INVALID_INPUT (unknown mode), MEETING_RUNNING, SETUP_INCOMPLETE, START_CHECK_FAILED (a meeting app id that is empty, longer than 64 characters or holds a control character fails the "meeting app chosen" check), NOT_IMPLEMENTED (default build: nothing starts) |
| stop_meeting | none | `{ meetingId }` | NO_MEETING |
| set_mute | `{ direction: "out" \| "in", muted }` | nothing | NO_MEETING (no meeting runs), NOT_IMPLEMENTED (a meeting runs; there is no audio path to mute yet, so it never answers Ok) |
| list_meetings | none | `MeetingSummary[]`, newest first | NOT_IMPLEMENTED (default build) |
| get_settings | none | `AppSettings` | NOT_IMPLEMENTED (default build) |
| set_setting | `{ key, value }` | nothing | INVALID_INPUT, NOT_IMPLEMENTED (input is valid, nothing is stored yet) |
| list_notes | none | `Note[]`, newest first | NOT_IMPLEMENTED (default build) |
| add_note | `{ text, tag }` | `Note` | INVALID_INPUT, NOT_IMPLEMENTED (input is valid, nothing is stored yet) |

Behaviour rules:

- `download_model`: `id` is a `ModelInfo.id`, which equals the `SetupItemId` of
  the model: `"sr"`, `"tr"`, `"enV"` or `"viV"`, spelled exactly. Both fields are
  required and no other key is accepted. Refusals run in this order: an unknown id
  (also `"mic"`, `"SR"`, `""`) returns INVALID_INPUT with `Unknown model: <id>.`;
  `licenceAccepted: false` returns INVALID_INPUT with the message
  `Accept the licence first.`; a known id with the licence accepted returns
  NOT_IMPLEMENTED until downloads exist. The shell resolves the id against its
  compiled-in model table and never accepts a URL from the webview (D-52). The UI
  sends `licenceAccepted: true` only after the owner ticked the licence box for
  that model.
- `cancel_model_download`: `id` rules as for `download_model`. An unknown id returns
  INVALID_INPUT with `Unknown model: <id>.`; a known id returns NOT_IMPLEMENTED
  (`Cancelling a model download is not available yet.`) until downloads exist.
- `start_meeting`: `mode` is sent as a string. The shell parses it, so an
  unknown value (also `"Both"`, `""`) returns INVALID_INPUT with a message, never a
  transport error. Refusals run in this order: INVALID_INPUT (mode),
  MEETING_RUNNING, SETUP_INCOMPLETE, START_CHECK_FAILED. The first one that applies is
  returned; nothing starts and no event is sent. `meetingAppId` and both `checks`
  fields are required; an unknown key inside `checks` is rejected.
- Per-meeting checks (START_CHECK_FAILED), decided by the owner at every Start. A check
  fails when:
  - `headphones`: the output device is the built-in speakers and
    `checks.headphonesConfirmed` is false;
  - `meetingMic`: `checks.meetingMicConfirmed` is false (the meeting app's microphone
    must be the Interlo virtual microphone);
  - `meetingApp`: `meetingAppId` is blank after trimming;
  - `routing`: the capture route and the English-voice output would share one virtual
    device (a fact from the shell, never an argument).

  The message is `Confirm before starting: <list>.` where `<list>` joins, in the same
  fixed order and with ", ", the texts of the failed checks: `headphones in use`,
  `meeting app microphone set to Interlo`, `meeting app chosen`,
  `capture and voice output on separate devices`. `details` holds the failed ids.
  The UI shows `headphones` and `meetingMic` with a confirm control and the other two as
  fix steps.
- `set_mute`: returns NO_MEETING when no meeting runs. The mute state is not stored yet.
  `direction` is typed: a value other than `"out"` or `"in"` is rejected by the
  bridge as a plain string error, not an `IpcError`. The UI never sends one.
- `set_setting`: INVALID_INPUT for an unknown key or a wrong value shape.
  `autoDeleteDays` must be `null` or an integer from 1 to 4294967295; `0`, a
  negative number, a fraction and a string are INVALID_INPUT.
- `list_meetings` is ordered by `startedAtMs`, newest first. `list_notes` is
  ordered by `createdAtMs`, newest first.
- `add_note` refuses with INVALID_INPUT when: `text` is blank after trimming; `text`
  is longer than 5000 characters after trimming (characters, not bytes); `tag`
  contains whitespace between non-space characters (for example `"two words"`);
  `tag` is longer than 64 characters after normalizing. Tag rule (the same as
  `normalize_tag` in `crates/workspace-notes`): trim, drop leading `#`, lowercase. A
  missing, blank or `#`-only tag means no tag (`tag: null`). The returned `Note`
  holds the trimmed text and the normalized tag.
- Ids (`meetingId`, `Note.id`) are unique, also when two are made in the same
  millisecond. Treat them as opaque strings.

SETUP_INCOMPLETE message: `Finish setup first. Not turned on yet: <groups>.`
where `<groups>` is `SetupStatus.missingGroups` joined with ", " (never empty when
the error is returned), for example `Finish setup first. Not turned on yet: Microphone, Models.`

## Types

```ts
type SetupItem = {
  id: "mic" | "capture" | "vmic" | "phones" | "sr" | "tr" | "enV" | "viV";
  group: "devices" | "virtualMicrophone" | "models";
  state: "ok" | "missing" | "installing" | "error";
  detail: string;
};
type SetupStatus = { items: SetupItem[]; canStart: boolean; missingGroups: string[] };
// One rule for every mode: canStart needs mic, capture, vmic, sr, tr, enV and viV
// all in state "ok". "phones" never blocks; it is a per-meeting check.
// Item groups by id: devices = mic, phones, capture;
// virtualMicrophone = vmic; models = sr, tr, enV, viV.
// missingGroups holds display names, in this order, only for required items that
// are not "ok" or are absent from `items`: "Microphone" (mic), "Meeting audio"
// (capture), "Virtual microphone" (vmic), "Models" (any of sr, tr, enV, viV).
// An empty `items` list gives all four names. canStart is true exactly when
// missingGroups is empty.

type StartMeetingArgs = {
  mode: "off" | "listen" | "speak" | "both";
  meetingAppId: string; // identifier of the chosen meeting app; blank fails "meetingApp"
  checks: { headphonesConfirmed: boolean; meetingMicConfirmed: boolean };
};

// ModelInfo.id is the SetupItemId of the model: "sr" | "tr" | "enV" | "viV".
// licence is a short line for the download dialog; nonCommercial is true when the
// licence forbids commercial use. Both are required.
type ModelInfo = {
  id: string; name: string; sizeBytes: number;
  state: "ok" | "missing" | "downloading" | "error";
  licence: string; nonCommercial: boolean;
};
type DownloadModelArgs = { id: string; licenceAccepted: boolean };
type CancelModelDownloadArgs = { id: string };
type ModelsProgressEvent = {
  id: string; receivedBytes: number; totalBytes: number;
  state: "downloading" | "verifying" | "ok" | "error";
};
type MeetingSummary = {
  id: string; title: string; startedAtMs: number; durationSec: number;
  status: "ready" | "generating" | "interrupted" | "failed";
};
type AppSettings = { uiLanguage: "en" | "vi"; ducking: boolean; autoDeleteDays: number | null };
type Note = { id: string; text: string; tag: string | null; createdAtMs: number };
```

## Events

| Event | Payload |
|---|---|
| `meeting://caption` | `{ lineId, direction: "out" \| "in", isFinal, source, target }` |
| `meeting://latency` | `{ p50Ms, p95Ms }` |
| `meeting://status` | `{ state: "idle" \| "starting" \| "running" \| "stopping" }` |
| `meeting://banner` | `{ kind: "info" \| "warning" \| "error", message }` |
| `models://progress` | `{ id, receivedBytes, totalBytes, state: "downloading" \| "verifying" \| "ok" \| "error" }` |

Event rules:

- `meeting://status` `{ state: "running" }` is sent after `start_meeting` succeeds;
  `{ state: "idle" }` after `stop_meeting` succeeds. A failed command sends nothing.
  `starting` and `stopping` are not sent yet.
- `meeting://caption` is coalesced per 50 ms window: the shell sends at most one event
  per `lineId` in a window, the latest one, in the order the lines first appeared. A
  line that is final is never replaced by a later event with the same `lineId`.
  The UI must still accept several updates for one `lineId` across windows, and
  must not rely on the exact window size. Captions are not wired to an engine yet.
- `models://progress` reports one model download: `id` is a `ModelInfo.id`, the byte
  counts are integers, `state` is one of the four names above. Downloads are not
  wired yet, so the shell sends no such event; the UI must still handle it and
  ignore an id it does not know.
