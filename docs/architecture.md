# Architecture (code map)

This file maps the decided architecture to the code. The full architecture page
(diagrams, routing, states) is linked in [decisions.md](decisions.md).

## Process model

One app process. The Rust core runs inside the Tauri shell. Engines run on worker
threads with ONNX Runtime in process; a helper process only if the M0-19 memo
shows a model cannot load in process (D-14). The UI is a webview that talks to
the core through typed commands and event channels only. The device list reads
the system audio tool as one short-lived child process with a reader thread, and
stops it after 5 seconds.

```text
Meeting app audio --(process tap, or 2ch virtual device)--> audio-io --> engine-host
Owner microphone ---------------------------------------->  audio-io --> engine-host
engine-host --> meeting-core (coordinator, journal) --> store / summary-jobs
engine-host --> playback (queue, mixer) --> audio-io --> virtual microphone (16ch) --> meeting app
                                                     --> owner's headphones
meeting-core --(events)--> apps/desktop (Tauri bridge) --(channels)--> apps/ui
```

`apps/desktop` also depends on `playback`, only for `playback::Direction`: the
wire type `ipc::Direction` converts to and from it (ADR 0020).

## Crates

| Crate | Owns | Threads | Must never |
|---|---|---|---|
| meeting-core | Protocol, session states, coordinator (line state), journal, policy gate | One actor task for the coordinator; one journal writer | Store audio; rewrite a final line; let text leave the Mac by default |
| audio-io | Devices, device list and setup report (eight items, typed fixes, Bluetooth warning), capture plan (process tap, then 2ch virtual device, else refuse), microphone capture, callback guards, output, ring buffers, resampling | Real-time callback plus one router thread; a short reader thread while the device list runs | Allocate, lock or log in the callback; block capture when a consumer is slow |
| engine-host | Engine traits (Vad, Recognizer, Translator, Synthesizer, SpeakerId, Summarizer), registry checked by the privacy policy at registration and at every use, test mocks (feature `mock`), segmenter, language gate, the voice activity adapter (`OnnxVad`, in process) and its start and end events (`SpeechDetector`) | One worker thread per model | Open a network connection; hand out an engine the policy refuses |
| playback | Single playback queue, speed-up up to 1.15x, play once per segment, unplug hold, mixer and ducking | Real-time callback for the mix | Cut, skip or reorder a segment; send the partner's original audio into the meeting |
| store | SQLite projections, schema, full-text index, cascade delete | One writer, background reads | Store audio |
| summary-jobs | Job queue (3 attempts, retry 30 s and 60 s, 3-minute lease), citation check, question answerer | Background task | Replace good notes with a bad reply; keep a point without evidence line ids |
| model-store | Compiled-in model table, size caps, checksums, download gate | Background download task | Run a model that failed its checksum; download without the owner's click |
| settings | Settings values, when each applies, hotkey map and conflict check | UI thread through the bridge | Store API keys outside the Keychain; accept a system key |
| diagnostics | Log events, timings, error codes, redacted export | Background writer | Write transcript text unless debug logs are on |
| workspace-notes | Notes (one tag), tasks, drafts from meetings, export, auto-delete (E1) | One store writer | Send notes or tasks to a network service |
| quick-actions | Translate, fix grammar, save note; guard; preview, then paste only after Enter (E2, D-45) | Hotkey thread; engine call on a worker | Read a password field; log keystrokes; paste without Enter |
| updater (planned, M4-15; the code location is decided in that task) | Update check, background download, signature check with the updater's own key, the "Restart to update" state (D-61, ADR 0015) | Background task | Check or download while a meeting runs; restart the app during a meeting; send more than the request for the manifest file; install an update with a wrong signature; open a connection outside the policy-owned client |
| driver installer (planned, M1; the code location is decided in that task) | One-press install of the virtual audio driver: native confirmation, pinned download, SHA-256 check, administrator prompt, device wait, loopback test (D-69, ADR 0016) | Background task; the shell shows the dialogs | Bundle or redistribute the installer; take a URL from the webview; run an installer that failed its hash; see the administrator password |
| integration-tests | Cross-crate tests only: capture, journal replay, pipeline, records, settings, delete through both stores | Test threads | Ship code, or hold logic that belongs to a crate |
| interlo-desktop (apps/desktop/src-tauri) | Tauri bridge: commands for control, channels for streams | Async handlers | Send settings or keys to the webview; allow broad capabilities |

## Audio adapter boundary

The engine reaches audio through two traits in `audio-io` (D-167), so a source
or a sink can change without a change in the engine or in the playback queue.

```text
CaptureSource (process tap, 2ch virtual device, later others)
  --push_slice--> ring buffer --> chain --write_frames--> AudioOutput
                                                (virtual microphone, headphones, ...)
```

- **Source contract (`CaptureSource: Send`).** `start` takes the ring producer.
  The source pushes mono f32 at `TARGET_SAMPLE_RATE` with `push_slice` only,
  from any thread; a source that captures at another rate converts inside
  itself. `start` returns without waiting for the consumer, and a full ring drops
  the newest samples and counts them. `start` on a source that cannot run
  returns a `CaptureError`. After `stop` returns, the source pushes nothing
  more. A real-time callback allocates nothing: rate conversion uses scratch
  space set up before `start`.
- **Sink contract (`AudioOutput: Send`).** `write_frames` returns how many
  frames it took, fewer than given is allowed, and the caller writes the rest
  again. A non-empty write that takes nothing returns `OutputError::Stalled`,
  never `Ok(0)`; the caller owns the retry policy. `write_frames` runs on a
  control-side worker thread, never in an audio callback. `flush` ends a stream.
- **Adapters now.** The process tap and the 2ch virtual device are the two
  decided routes (`capture_plan`). The microphone is a third source,
  `MicCapture` (M1-06): it opens an input device by exact name, mixes it down to
  mono and converts it to 16 kHz inside the audio callback, and pushes to the
  ring. The callback runs inside `realtime::section`; a debug build counts an
  allocation (`GuardAllocator`) or an `RtMutex` lock made there. The process tap
  is still a placeholder (M1-34).
- **A new source** is one implementer of `CaptureSource`, one `CaptureMethod`
  variant and one line in `capture_plan`. The engine and the playback queue do
  not change.
- **Playback side.** `SegmentPlayer` stays in `playback`; it carries no samples
  and `playback` has no audio backend. A player that writes to an `AudioOutput`
  is the next step (M1-08, M1-12).
- `crates/audio-io/tests/adapter_boundary.rs` swaps fake sources and sinks
  behind the two traits and checks the chain output on a fixture.

## UI features (apps/ui/src/features)

Rows say what exists in the skeleton. Text marked "planned" is not built yet.

| Feature | Screen |
|---|---|
| shell and navigation | Sidebar Home, Live meeting, Meetings, Setup with badges (Live dot while running, Meetings count, Setup missing-count badge); back-to-Meetings chip; Settings sheet (five pages) from the Settings button and Cmd+comma. Planned; the wireframe is the spec (D-62) |
| command bar | Cmd+K palette: filter as you type, Enter runs the first item, Esc closes; go-to items, start and stop, settings, mute, partner voice, subtitle bar, compact window, appearance, language pair, one "Open <meeting title>" per saved meeting. Planned, M4 |
| home | Ready card, recent meetings. The mode switch lives on Live today. Planned (D-67): greeting, readiness, mode picker, Start meeting, Review setup, This Mac card, Recent meetings; E1 adds a Today section above Recent meetings |
| setup | Virtual microphone group, Models group, permissions, check and finish |
| live | Mode switch, mute and pause controls, status, captions. Two direction lanes, partners and notes are planned |
| meetings | History list and title search. Filters and delete are planned |
| meeting-detail | Overview, summary status (generating, ready, failed), transcript. Notes, action items and export are planned |
| settings | General, models; voices, vocabulary and hotkeys tabs are static until their commands exist |
| workspace | Daily workspace: today, notes, search (E1). The tasks tab is a placeholder |
| quick-actions | Preview panel for translate, grammar, save note (E2) |
| subtitle-bar | Latest caption line (source and translation) over the meeting; a partner-only filter is planned |
| compact-panel | Small window with captions and stop. Transcript, notes and mute are planned |

## Meeting controls

- **Pause** (D-65). Stops translation in both directions. Partners hear silence,
  the clock stops, captions and audio are held. Resume continues. Stop works
  while paused. Off mode is different: no translation, no captions, no voice,
  and the meeting is still saved (D-64).
- **Language pair** (D-63). The top bar shows SPEAKS and HEARS and a swap
  button. v1 supports Vietnamese and English only. Swap reverses the direction of
  both chains: the owner speaks English and hears Vietnamese.
- **Replay** (D-66). The Play button on a transcript line re-speaks the stored
  text with the current voice. No audio is stored.

## Network use (egress rule)

Network only for a model download or a driver installer download the owner
starts, and the update check outside meetings (guardrail 1, D-61, D-69). Every
call goes through the policy-owned client (ADR 0007); the model host, the update
host and the driver host are on the policy allow list. The update check sends
only the request for the manifest file.

Updater flow (ADR 0015):

```text
app start or every 24 h, or "Check for updates"
  -> meeting running? yes: do nothing (check later)
  -> fetch the manifest (policy client)
  -> newer version? no: stop
  -> "Update automatically" off: tell that a new version exists, download nothing
  -> download in the background (never while a meeting runs)
  -> verify the signature with the updater's own key; wrong signature: refuse
  -> show "Restart to update" (the app never restarts while a meeting runs)
```

Driver installer flow (ADR 0016), one press of Install at Setup:

```text
native confirmation (name, source, size)
  -> download from the pinned URL (policy client)
  -> check SHA-256 against the manifest; mismatch: refuse, nothing runs
  -> run the installer with the macOS administrator prompt
  -> wait up to 15 s for the device
  -> loopback test phrase, then run the setup check again
  -> say in plain words if an audio service restart or a restart of the Mac is needed
```

## Data model

Tables: meetings, utterances, speakers, summaries, insights, notes, tags, tasks,
jobs, models, settings, the journal events table (meeting-core) and the notes
full-text index. Deleting a meeting removes its rows
in the store (cascade) and its journal rows (`Journal::delete_session`, ADR 0008).
Notes created in a meeting are deleted with it. Daily notes not tied to a meeting
stay. Confirmed tasks stay with the meeting link cleared; unconfirmed drafts from
that meeting are deleted (D-46, D-53).

Database and time rules (D-59, ADR 0013):

- The journal sets `PRAGMA user_version` (version 1) and refuses to open a
  database with a higher version.
- Dates and times are stored as UTC milliseconds and shown and exported in the
  user's local time zone at the moment of use.
- The schema may change freely until the first public tag; from then on every
  schema change needs a migration.
- `Store::conn()` becomes crate-private before E1 (task E1-01).

Models: the model manager in `crates/model-store` downloads from a pinned
manifest (URL and SHA-256), shows progress on `models://progress`, and stores
files under `~/Library/Application Support/Interlo/models` (ADR 0012).

## Start checks

Two layers (D-44, ADR 0009). Layer 1 is readiness: `check_setup` sets `canStart`
from the microphone, meeting-audio capture, the virtual microphone and the four
models. Layer 2 is the per-meeting checklist that the owner confirms at every
Start: headphones, meeting app microphone, meeting app and routing.
`start_meeting` refuses with `SETUP_INCOMPLETE` or `START_CHECK_FAILED`.

## Build order

M0b shell first, then M1 (outgoing chain), M2 (incoming chain, speakers, mixer),
M3 (records, summary, history), M4 (settings, subtitle bar, signing), then E1 and
E2 in the same v1 release (D-43).
