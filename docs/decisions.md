# Decisions that bind the code

The Decision Log is the full record. This file lists the decisions a coding task
most often needs. If a line here and the Decision Log disagree, the Decision
Log wins; fix this file.

## Planning pages

| Page | Link |
|---|---|
| PRD | https://claude.ai/artifact/RYJLXrmLhc5hyTVCzLqVom |
| Roadmap | https://claude.ai/artifact/YZfgsC3hzo4SJeLMtLXyew |
| Task Breakdown | https://claude.ai/artifact/GXAdu1pvdaFofUb1FLpHzC |
| Architecture | https://claude.ai/artifact/BD3goGkCGTa1ioN2655XyF |
| Decision Log | https://claude.ai/artifact/QWFhFBeTu5SQKq7po9qrUo |
| Gap Register | https://claude.ai/artifact/MhDAUjYCKPvCx6bC9HmMcb |
| Wireframe | https://claude.ai/artifact/MgLQKCFQgTfwWRf8ZV7gFT |
| Design System (tokens and components; see `design.md`) | https://claude.ai/artifact/CpPMyvnhc34epKE8QXDjBi |
| Desktop App demo | https://claude.ai/artifact/7Fhv5F5UpXwm87w1VVig3w |
| UI Foundations | https://claude.ai/artifact/Nif3R9Bghoeb4rfJv8H8Xp |

These pages are owner-only links; contributors read the repository docs. The Decision Log content that binds code is mirrored in this file.

TODO(owner), before the repository is public: commit the wireframe export under `docs/wireframe/` (ADR 0017 makes it the final UI spec) and remove these links.

## Architecture decision records

| ADR | Decision |
|---|---|
| [0001](adr/0001-rust-core-tauri-shell-react-ui.md) | Rust core with a Tauri 2 shell and a React UI |
| [0002](adr/0002-local-only-no-cloud-in-v1.md) | Local only, no cloud in v1 (D-42) |
| [0003](adr/0003-mit-licence-models-not-bundled.md) | MIT licence, models not bundled (D-39) |
| [0004](adr/0004-developer-id-signing-and-notarization.md) | Developer ID signing and notarization for releases (D-40; superseded by 0022) |
| [0005](adr/0005-macos-only-in-v1.md) | macOS only in v1 (D-41) |
| [0006](adr/0006-model-download-confirmed-by-native-dialog.md) | Model download is confirmed by a native dialog in Rust, not by the webview (accepted, D-52) |
| [0007](adr/0007-network-access-only-through-policy-owned-client.md) | Network access only through a policy-owned client (part 1 accepted, D-48; part 2 proposed, M4) |
| [0008](adr/0008-deleting-a-meeting-deletes-journal-rows.md) | Deleting a meeting deletes its journal rows and uses secure delete (D-46) |
| [0009](adr/0009-start-checks-two-layers.md) | Start checks run in two layers (D-44) |
| [0010](adr/0010-quick-actions-enter-only.md) | Quick actions paste only after Enter (D-45) |
| [0011](adr/0011-models-with-non-commercial-data-in-default-list.md) | Models trained on non-commercial data stay in the default list (D-47) |
| [0012](adr/0012-model-manager.md) | Model manager: pinned URL and SHA-256 manifest, models from their own public host, no Interlo server (accepted, D-60) |
| [0013](adr/0013-database-and-time-rules.md) | Database and time rules: journal `user_version`, UTC milliseconds, schema free until the first public tag (accepted, D-59) |
| [0014](adr/0014-contribution-policy.md) | Contribution policy: DCO sign-off, no CLA, protected main (accepted, D-57; code of conduct removed 2026-11-19, D-187) |
| [0015](adr/0015-automatic-updates.md) | Automatic updates: checked outside meetings, signed with its own key, from GitHub Releases (accepted, D-61) |
| [0016](adr/0016-driver-installer.md) | Driver installer: one press, pinned URL and SHA-256, never bundled, administrator prompt (accepted, D-69) |
| [0017](adr/0017-wireframe-is-final-ui-spec.md) | The wireframe is the final UI spec (accepted, D-62) |
| [0018](adr/0018-updater-never-downgrades.md) | The updater never installs a lower version (accepted, D-79) |
| [0019](adr/0019-additive-migrations-one-version-back.md) | Database migrations are additive and the previous version can read them (accepted, D-80) |
| [0020](adr/0020-ipc-types-stay-hand-written.md) | IPC types stay hand-written; the fixtures keep both sides in step (accepted) |
| [0021](adr/0021-downloads-https-only-redirects-inside-the-allow-list.md) | Downloads use https only and follow a redirect only inside the pinned allow list (accepted, D-175) |
| [0022](adr/0022-release-without-apple-developer-id.md) | Release without an Apple Developer account: a locally signed DMG, no notarization, an install guide (accepted, D-188) |

## Scope and release

| ID | Decision |
|---|---|
| D-39 | Code licence MIT. Models are not bundled. |
| D-40 | Release builds: Developer ID certificate and notarization. Local builds: self-made certificate. The earlier "no automatic update" is superseded by D-61. Superseded by D-188 (2026-11-19): a release is a locally signed DMG, no Developer ID, no notarization. |
| D-188 | Release without an Apple Developer account (owner statement, 2026-11-19). A release is a DMG signed on the owner's Mac with the local self-made certificate (H-10), published with a SHA-256 checksum. It is not notarized and not in a store. The README has an install guide: the first open shows a macOS warning and the user allows the app in System Settings, Privacy & Security (Open Anyway). The updater keeps its own key (D-61, H-21). The goal "no Gatekeeper warning" no longer holds. Not verified yet: permissions stay after an update when every release uses the same certificate, and an update that the app installs shows no new warning (M4-20 and the M4 smoke test). See ADR 0022. Supersedes D-40. |
| D-41 | macOS only in v1 (Apple silicon, macOS 14.4 or later). |
| D-42 | No cloud in v1: no cloud engines, accounts, sharing or backend. |
| D-43 | Daily workspace (E1) and quick actions (E2) ship in v1, after the MVP gate, in the same app as modules. |
| D-47 | Models trained on non-commercial data stay in the default list. The model manager shows each licence and its non-commercial flag; the owner accepts before the download. README and NOTICE say so. |
| D-56 | The README and every other repository text, UI text, code, test and fixture name only Interlo. Libraries and models are named only in `docs/tech-stack.md` and `NOTICE` as dependency attribution. The UI uses generic labels for meeting apps. Guardrail 3 stays as written. |
| D-57 | Contribution policy: DCO sign-off (`Signed-off-by`), no CLA; main branch protected with the required CI check and one review (owner action H-20). Task X-12 adds the files and the DCO check. The code of conduct part is superseded by D-187: `CODE_OF_CONDUCT.md` was removed on the owner's request (2026-11-19). See ADR 0014. |
| D-58 | Nothing is sent automatically. "Export diagnostics" writes a redacted file (timings, event names, error codes; no transcript text) that the user attaches to an issue by hand. The update half of this decision is superseded by D-61 (automatic updates, ADR 0015); the diagnostics half stays. |
| D-54 | The README keeps the sentence "You must follow the recording laws where you live." There is no consent notice in the app (M-Q11). |
| D-55 | Default hotkey for mute my voice is Ctrl+Option+M (a single modifier with M types a character on some layouts). Every default hotkey uses Control plus Option (see Hotkeys below). Note (D-62 round): Ctrl+Option+M now mutes both directions; the start-meeting key is removed. |
| D-48 | ADR 0007 part 1 accepted: a network client only the policy can create, plus a CI check that no other crate opens a socket. Part 2 (no outbound entitlement in the main app, helper process, App Sandbox) is deferred to M4 and must first be checked against the Accessibility API that quick actions need (E2). |
| D-50 | Gate numbers for M1. "Unusable" recognition: word error rate above 20 percent on the owner's Vietnamese reading list (a starting value; change it after the first real M0-05 numbers, with a Decision Log row). "End of sentence" is the time the voice activity detector reports end of speech; "first sound" is the time the first sample of the English voice is written to the 16-channel virtual device; both are logged on one session clock by the harness (M1-26); end to end is first sound minus end of sentence. The gate at the end of M1 measures the outgoing chain only; the incoming chain is re-measured at M2-16 and by M0-23 after any fallback step. The gate uses the owner's own voice only; other accent groups (H-04) are measured before M4. p95 and p50 stay as in D-33. See `docs/m0/measurement.md`. |
| D-33 | Latency gate measured at the end of M1 with live speech: p95 under 2.0 s from end of sentence to first sound; p50 goal under 1.0 s. |

## Updates, installer and UI (round 4)

| ID | Decision |
|---|---|
| D-61 | Automatic updates in v1. Check at app start and every 24 hours, never while a meeting runs; "Check for updates" in About checks at once, also never during a meeting. Download in the background, verify the signature, then show "Restart to update"; the app never restarts during a meeting. Settings switch "Update automatically", default on; off means the app only tells that a new version exists and downloads nothing. Source: GitHub Releases of the Interlo repository, signed with the updater's own key (separate from the Apple Developer ID certificate) and notarized. The update host is on the policy gate allow list. Sent data: only the request for the manifest file; no machine id, user id or usage data. Built at M4 (M4-15, X-10); the key is owner task H-21. Reverses the update half of D-58. See ADR 0015. Notarization part superseded by D-188. |
| D-62 | The wireframe is the final UI spec. Where the PRD, task breakdown or roadmap differ, they change to match it, except where a later decision says otherwise. Wireframe demo helpers (Demo: buttons and toasts, scripted call, sample data) are not product. See ADR 0017. |
| D-63 | Language pair. The top bar shows SPEAKS and HEARS and a swap button; the "Language pair" dialog says "v1 supports English and Vietnamese only." Swap: the owner speaks English and hears Vietnamese, and both chains reverse direction. The v1 pair set is Vietnamese and English only, direction switchable. |
| D-64 | Off mode: no translation, no captions, no voice. The meeting is still saved (timer, notes). |
| D-65 | Pause: stops translation in both directions; partners hear silence; the clock stops; Resume continues; Stop works while paused. Text: "Captions and audio are held". See D-72 for what happens to speech during Pause. |
| D-66 | Replay: the Play button on a transcript line re-speaks the stored text with the current voice. No audio is stored. "Play the partner's own English" works only while the meeting runs (live buffer, not stored). |
| D-67 | Home as in the wireframe in M1 to M4 (greeting, readiness, mode picker, Start meeting, Review setup, This Mac card, Recent meetings). E1 adds a "Today" section (tasks, notes, today's meetings) above Recent meetings. Rewords D-20. |
| D-68 | Meeting app at Start: two generic choices. "Meeting app in the browser" is the browser meeting app (dedicated browser profile, FR-68). "Meeting app (desktop)" is the desktop meeting app that is playing audio, found automatically. No extra menu. |
| D-69 | Driver installer: Install does the whole job in one press. Native confirmation (name, source, size, as in D-52); download of the official installer from the driver's own public release (pinned URL, SHA-256 in the manifest in the repository); never bundled or redistributed; run with the macOS administrator prompt (the app never sees the password); wait up to 15 s for the device; loopback test phrase; setup check again; plain words if an audio service restart or a restart of the Mac is needed. No manual Terminal steps. The "guided steps" of D-10 are superseded by this one-press install; D-10 keeps "not bundled". See ADR 0016. |
| D-70 | Interface language has English and Vietnamese (M4-02 stays). "My English voice" lists male and female presets with a gender tag; the three voices in the wireframe are examples. |
| D-71 | Language pair during a meeting: Swap works only when no meeting is running. During a meeting the button shows the toast "Stop the meeting to change the language pair." |
| D-72 | Pause and speech: speech during Pause is not translated and not stored. The transcript gets one marker line "Paused 00:41". Resume starts from that moment with no catch-up queue. |
| D-73 | Mute both directions (Ctrl+Option+M): mutes the owner's voice and sets the partner voice to the original English. A second press restores the earlier mute state and partner voice. |
| D-74 | Network disclosure: the Home, Setup and sidebar strings stay as in the wireframe. The About and Settings privacy text lists all three network uses: model download, driver installer download (both started by the owner) and the update check outside meetings. |
| D-75 | Discovery depth: phase 01 is a lightweight Strategy page with validation experiments V-1 to V-5. No market sizing and no pricing research, because the app is free and open source. |
| D-76 | Telemetry: Interlo sends nothing about its use to anyone. Problems are learned from GitHub issues and the diagnostics file the owner exports by hand (D-58). |
| D-77 | Capacity: one owner, about 8 hours a day with a coding assistant (read as 8 hours a day, 5 days a week). The statement of work converts task sizes to hours and weeks from this and is calibrated after sprint 3. |
| D-78 | The owner approved the statement of work (scope baseline B0: 214 tasks, requirements FR-01 to FR-83). Effort and schedule are estimates, calibrated after sprint 3. Changes go through change control. |
| D-79 | The updater never installs a lower version. A bad release is withdrawn and replaced by a higher fixed version. See ADR 0018. |
| D-80 | Database migrations are additive; the previous version must open a database written by the current one. A preview build shared with a colleague counts as the first shared build. See ADR 0019. |
| D-81 | Preview channel: preview builds are shared as DMG links, never listed in the update manifest, with no Settings option. |
| D-82 | Debug logs switch off when the app quits. Export diagnostics refuses while debug logs hold transcript text. |
| D-83 | Success for v1: the latency gate (V-1) and use in real calls (V-2) pass. Outside users are a target for v1.0 (amended by D-110). Until M4 the owner is the only user who decides go or no-go. |
| D-84 | Real calls: Interlo is used in a real call only after M2-16 passes, with a fallback ready (interpreter or the old workaround). Partners are asked three questions by voice after a call (V-3); nothing is recorded. |
| D-85 | The trademark register search (H-11) is done before the first outside user. Whether it is already done is unconfirmed. Superseded by D-96. |
| D-86 | Usage correction: the owner takes about 5 calls a week with English-speaking partners, not 10. Call length and partners per call are unchanged. V-2 needs 10 real calls, about two weeks of use. |
| D-87 | Problem evidence (SW7-Q1, owner statement). Of the owner's about 5 calls a week, in 1 to 2 the owner cannot say what they mean to the client and stammers through it. No interpreter is hired today and no money is spent; the cost is clarity and confidence in those calls. This replaces the Unknown on "what it costs today" in the Strategy page. |
| D-88 | Call shape to focus on first (SW7-Q2, owner statement). All first measurements and acceptance runs cover two cases: the owner with one client (1:1) and the owner with two clients (1:2). In a meeting people speak in turn: one person speaks, then the next. Overlapping speech is outside the v1 acceptance tests. The 9-partner limit stays in the design, but no pass line applies above two partners until after M4. The aim is to get a result first. |
| D-89 | Contrast fix (SW7-Q3). The two failing "live" colour pairs in the light theme are darkened to at least 4.5:1. Only the token values change; layout and text in the wireframe stay (task X-50). |
| D-90 | Gate-first order (SW7-Q4). The tasks the latency gate (M0-20) needs run first, so the gate falls near sprint 9. The remaining M1 screens (Home, setup, driver installer) run beside M2 work. Nothing from M2 on starts before the gate. This refines D-33 and the Sprint Plan. |
| D-91 | Capacity confirmed (SW7-Q5). 8 hours a day, 5 days a week, 70 percent usable, which is 28 planning hours a week (D-77). Calibrated after sprint 3. |
| D-92 | Recording-law line (SW7-Q6). The "Before you start" dialog (FR-69, FR-72) gets one more line that reminds the owner to follow the recording laws where they live. No consent flow; M-Q11 stays. |
| D-93 | Updater key custody (SW7-Q7). The updater signing key lives in a hardware-backed store with an offline backup, not only the login Keychain. Owner task H-21 and control X-14 follow this. |
| D-94 | Release account (SW7-Q8). Hardware second factor and signed tags on the release account before the repository becomes public. Owner task H-20 and control X-14 follow this. |
| D-95 | Second reviewer (SW7-Q9). The owner has no second reviewer yet. H-06 stays open and the person is named before sprint 30 (before M4-10). |
| D-96 | Name is final (SW7-Q10). The owner decided the name Interlo is final and that no trademark register search is needed. This supersedes D-85. H-11 is dropped. The similar registered name noted in Roadmap section 9 is a risk the owner accepted. |
| D-97 | PRD edit rule (SW7-Q11). Clear defects (wrong numbers, version labels, contradictions with a decision already made) are fixed in the PRD and listed afterwards. Points that need a choice go to the owner or the Gap Register. |
| D-98 | Incoming chain gate (SW7-Q12). The incoming chain has a pass line: p95 under 2.0 s on at least 30 sentences at M2-16, run 3 times, reporting all three values and the pooled value. If it fails, stop and report the numbers to the owner. The percentile method is named (nearest rank). |
| D-99 | Swap rules (SW7-Q13). FR-70, FR-35 and FR-36 and their banner texts are written in terms of "my language" and "partner language", so they stay correct after Swap. One extra gate run covers the swapped order, at M2-16. |
| D-100 | Speaker split pass line (SW7-Q14). One pass line everywhere (D-02, FR-06, Roadmap, M2-16): at least 90 percent of turns correct and no speaker split or merged in the test call with the owner and two partners. |
| D-101 | Accent and language-detection targets (SW7-Q15). Targets per accent group and for language detection are set after the first measured numbers (M0 and M2). Once set, the test fails below the target. |
| D-102 | Starting values for soak and ducking (SW7-Q16). 30-minute run: memory grows by at most 10 percent between minute 5 and minute 30, and no thermal throttling event. Ducking of the original voice: 12 dB down, 300 ms release, at most 50 ms added delay. Both are starting values, tuned after listening tests (M2-12). |
| D-103 | Meeting states and Interrupted recovery (SW7-Q17). Definitions: running means the session exists (including paused); paused is a running meeting with translation held; interrupted is a meeting that was running when the app stopped. Paused counts as running for FR-76 (no update restart) and FR-77 (Swap locked). New FR-84: on the next start, a meeting that was running is marked Interrupted, its journal lines are kept, and the owner can Open it or Write summary. |
| D-104 | Display default (SW7-Q18). Display defaults to "Live window". The Subtitle bar row only sets whether the bar is available (⌃⌥S, or Display set to Subtitle bar). |
| D-105 | Pause toast (SW7-Q19). The Pause toast reads "Paused. Nothing is translated or saved." and "Resumed." (replaces "Captions and audio are held"). |
| D-106 | Non-functional rules and text fixtures (SW7-Q20). Each non-functional rule in PRD section 5 has an id, a number and a check (for example contrast 4.5:1). Notes, quick translate and grammar use a fixture of 20 sentences; pass at 16 or more that keep the meaning (same ratio as G2). |
| D-107 | Existing-tool baseline (SW7-Q21). Experiment V-6 (task X-53): before M2, score 3 real calls in the meeting app that offers Vietnamese spoken translation (only if the owner has a plan that includes it) and 3 in the open project's on-device mode, with the same two self-scores as V-2. If an existing tool scores a mean of 4 or more on both, the owner decides whether to continue. The owner has not said which meeting app or plan they use; the first part is skipped if there is no such plan. |
| D-108 | Latency risk and market watch (SW7-Q22). Risk R-1 (latency gate) is High. A Vietnamese end-to-end spike (recognition, translation, voice) comes before any UI work. The Vietnamese language lists of the major meeting apps are checked every quarter until v1.0 and the result is recorded. |
| D-109 | Personal tool (SW8, owner statement 2026-11-17). Interlo is built as a personal tool to solve the owner's own problem. The owner's statement is enough evidence that the problem exists and who the user is. Outside demand is optional: the discovery chats (H-22), the colleague trial (V-4) and the search for demand beyond the owner are not required to start the build. The owner decides alone (D-83). The project may still be published as open source under the MIT licence, but outside use is not a goal. The existing-tool baseline (V-6) and the partner voice check (V-3) stay, because they test whether the tool solves the owner's own calls. Superseded by D-110. |
| D-110 | Production goal (SW9, owner statement 2026-11-17). Interlo starts from the owner's own problem, but the goal is a production-grade app that other people can install and use well. A release must meet a general-user standard: setup works without help (V-5), the SLOs and release criteria of the former Operations Plan (dropped, D-169) are met before each release, builds are signed and notarized, and updates, documentation and error messages are written for people who are not the owner. This supersedes D-109 and amends D-83: outside users are a target for v1.0. Until M4 the owner alone still decides go or no-go. Evidence from users other than the owner (the colleague trial V-4 and the discovery chats H-22) is needed before the v1.0 release; H-22 is recommended before M1-24. The build itself starts from the owner's problem. Amended by D-166: V-5 is dropped; G10 rests on V-4. |
| D-111 | Production standard confirmed (SW10-Q1). A release is production-ready when: the SLOs and release criteria in the former Operations Plan (dropped, D-169) are met; the build is signed with a Developer ID certificate and notarized; automatic updates work; first-time setup without help passes (V-5); and the colleague trial passes (V-4). Nothing more is added for v1: Apple silicon, Vietnamese and English only. This confirms D-110. Signing part superseded by D-188: the build is a locally signed DMG, not notarized. |
| D-112 | Code hosting (SW10-Q2). During development the repository is a private GitLab project with its own CI pipeline. When the app is stable, a public GitHub repository is created for the open-source release, and updates are served from GitHub Releases (D-61, ADR 0015). Preview builds before that are shared as DMG links (D-81). Task X-05 creates the private GitLab project and X-55 its CI pipeline; X-56 is the checklist for going public on GitHub, and X-11 (README) applies then. |
| D-113 | Discovery closed (SW11, owner decision 2026-11-17). Phase 01 is closed: the problem is stated by the owner (D-87), alternatives are researched, the production goal is set (D-110) and the experiments are defined. Open items are carried into the build, not kept in discovery: the existing-tool baseline V-6 (task X-53, before M2), the latency gate V-1 and the Vietnamese end-to-end spike (M1-08, M0-20), the discovery chats H-22 (recommended before M1-24) and the colleague trial V-4 (X-48, before the v1.0 release). |
| D-114 | Latency gate pass rule (SW12-Q1). Three runs of at least 30 sentences each. The gate passes when the p95 of the pooled sentences is under 2.0 s and all three runs are reported. A single run with p95 above 2.5 s is reported to the owner, who decides. The swapped-order run at M2-16 uses the same rule. This completes D-98 and D-99. |
| D-115 | Proposed numbers confirmed (SW12-Q2). Starting values: a debug-build assertion fails on an allocation in an audio callback (NFR-05); each borrowed file carries a licence line (NFR-09), and the PRD names no other open-source project; stable text means text unchanged over N recognition updates, with N set at M0-22; the latency chip matches the measurement log within 5 ms (FR-10); the FR-84 test kills the app after at least 3 final lines. |
| D-116 | Stop and save (SW12-Q4). FR-71 wins over the immediate stop shown in the wireframe. After "Stop and save" the app finishes the sentence in progress, waiting at most 5 seconds (starting value), then stops. Journey J2 and the wireframe get a "Finishing..." state. |
| D-117 | Microphone unplugged (SW12-Q5). New error DEV-04, "Microphone unplugged": the outgoing direction stops, the banner reads "Microphone disconnected. Your outgoing direction is stopped. Reconnect it or pick another microphone.", with the button "Retry microphone", and FR-12 gets a test for it. |
| D-118 | Reconnect of headphones (SW12-Q6). Queued audio plays automatically when the device is plugged in again (FR-67). The button "Retry output" is a manual fallback for when the app does not recognise the device (FR-12). Both FRs say so. |
| D-119 | Auto-delete setting (SW12-Q7). Meeting auto-delete ("Keep meetings") lives only on the Models and privacy settings page. It is removed from the General page. |
| D-120 | Milestone column (SW12-Q8). PRD section 4 gets a Milestone column: each requirement names the milestone whose task builds it, taken from the Task Breakdown. The owner reviews the list. |
| D-121 | Original voice (SW12-Q9). The behaviour "play the partner's original English" is described in FR-33 only. FR-05 states the default (Vietnamese). FR-09 covers mute and the mute-both key only. |
| D-122 | Missing journeys (SW12-Q10). docs/wireframe/user-flow.md gets J14 (daily workspace: Today, notes, tasks) and J15 (failure recovery: engine stopped, device unplugged) as step tables. |
| D-123 | Spike and model manager (SW12-Q11). M1-08 (the Vietnamese end-to-end spike) uses the core of the model manager of M1-04 (download and checksum), not its licence screen. No task changes. |
| D-124 | FR-06 merge test (SW12-Q3). The check "Merge last two with three partners" is a function test that the button works. It is not an accuracy pass line; the pass line of FR-06 is D-100. |
| D-125 | Database version rule (SW13-Q12). An older build opens a database written by a newer build when every migration is additive; a database is refused only when a migration marked it incompatible. The test opens a version-2 database with version-1 code. This corrects item 3 of D-59 and supports D-80. |
| D-126 | Baseline wording of D-78 (SW13-Q13). D-78 keeps its approved wording: requirements FR-01 to FR-83 and 214 tasks. FR-84 is a change request after approval (CR-2, D-103), recorded in the SOW; the PRD, Roadmap and Decision Log say so in the same words. |
| D-127 | FR-24 is unconditional (SW13-Q14). FR-24 (no translated audio when the speaker already uses the target language) always applies. The condition on M0-13 in M2-14 and the Roadmap is removed. If M0-13 fails, the fallback order of PRD section 9 applies; the requirement is not dropped silently. |
| D-128 | V-3 and D-84 (SW13-Q15). The first run of V-3 may take place on the M1-24 test call, a test with a person who knows it is a test and is not a client. A run of V-3 in a real call waits for M2-16, as D-84 says. |
| D-129 | FR-84 and NFR-13 test (SW13-Q16). The kill test uses at least 20 final lines and 5 forced kills at different moments, one of them while a line is being written. It passes when no final line is lost in any of the 5 kills. This replaces the 3-line value of D-115. |
| D-130 | Latency chip precision (SW13-Q17). The chip shows the same numbers as the measurement log after rounding to the chip precision. This replaces the 5 ms tolerance of D-115. |
| D-131 | The build matches the wireframe exactly (SW14-Q1, owner ruling). The type sizes and corner radii used in the wireframe are the design tokens of the build; none is merged or dropped. UI Foundations lists them as they are. Task X-50 moves the tokens into one tokens file without changing a value. This replaces the proposal in UI Foundations to reduce the scales. |
| D-132 | Accessibility standard for v1 (SW14-Q2). Text contrast at least 4.5:1 and focus ring at least 3:1 in both themes; full keyboard use; every icon-only control has a text label for VoiceOver; app zoom across its whole range (80 to 140 percent in the wireframe) without clipping; the system "Reduce motion" setting is respected. Nothing more is promised for v1. |
| D-133 | Captions and VoiceOver (SW14-Q3). New caption lines are not read aloud by VoiceOver by default. A setting "Announce new captions" is off by default. Failures and Stop are always announced. |
| D-134 | One database file (SW14-Q4). Meetings, journal, notes, tasks and settings live in one SQLite file. The store crate owns the single user_version and all migrations; the journal writes through the store connection. A task changes the code before the first shared build. D-125 applies to this file. |
| D-135 | Engine process model (SW14-Q5). In v1 the engines run as threads in the app process. A failing engine must not crash the app: it restarts within 3 s (FR-11). The memo M0-19 decides, from the memory and stability numbers, whether a model moves to its own process. |
| D-136 | Three transport and file requirements (SW14-Q6). (a) The update manifest is signed as a whole and fetched over https only. (b) An export path comes only from the system save dialog. (c) Every download of a model or installer comes from a pinned host, is hash-checked before use and never follows a redirect to another host. These are requirements for the controls X-13, X-15, X-16 and X-20. Item (c) is amended by D-175. |
| D-175 | Redirect rule for downloads (SW16-Q3; amends D-136 item c). A download of a model, a driver installer, an update manifest or an update package follows a redirect only to a host on the pinned allow list, only over https, within a fixed hop limit set in code and tested. The policy checks every hop and the hash of the final file is checked before use. A redirect to any other host fails the download and logs NET-01. See ADR 0021. |
| D-137 | Release blockers from the Threat Model (SW14-Q7). v1.0 is not released until controls X-13, X-14, X-15 and X-16 are done, each with evidence. They are listed as release blockers in the former Operations Plan (dropped, D-169). |
| D-138 | C4 labels (SW14-Q8). The Architecture page names its views Context, Container and Component in the matching headings. Wording only. |
| D-139 | Threat Model accepted (SW14-Q9). The owner read the Threat Model and accepts it, with the answers D-79, D-82, D-92 to D-94 and D-136, D-137. |
| D-143 | CI for macOS jobs runs on a self-hosted runner on the owner's Mac mini (SW15-Q4). Signing keys and certificates are never on the runner; release signing is done by hand on the owner's machine. Jobs that do not need macOS run on shared runners. | Decided 2026-11-18 |
| D-145 | Review level by task size (SW15-Q6). S: the coding assistant tests and reviews its own change. M: add review agents by aspect. L, and every task that touches the updater, signing, the driver installer or a security control: add an independent review by another model, and the owner approves the merge. | Decided 2026-11-18 |
| D-146 | No second human reviewer (SW15-Q7). H-06 is dropped. The owner is the only human reviewer; review levels (D-145) stand in. M4-10 no longer waits for H-06. | Decided 2026-11-18 |
| D-160 | Work model: the agent writes all code and tests. One merge request per feature (about 35 to 45 in all). Each merge request carries the test output, screenshots for UI changes, and the result of an independent review by another model. The owner reviews and merges within about 4 working hours; a waiting merge request blocks the tasks that need it. Commits and pushes still happen only when the owner asks. The owner still does the owner-only tasks (H) and the human parts: judging speech quality, calls with people, signing credentials. | Decided 2026-11-18 |
| D-167 | Focus (owner ruling, 2026-11-18). v1 is the desktop app only; no browser extension is built, and the extension idea stays in Roadmap phase P11. A new task M1-50 (size S, feature FT-07) puts the audio sources and sinks of the engine behind one adapter interface, so a source can be added later without changing the engine. An extension is reconsidered only if the process-tap spike M0b-05 shows that the browser meeting app cannot be captured reliably; then a one-to-two day spike is added and the owner decides at the end of day 3. Test apps: the browser meeting app at M1, desktop meeting app A at M2 and desktop meeting app B at M4 (D-18); the owner chose which apps these are, and their names are kept in the private Sprint Plan because shared pages use generic labels. The M2 and M4 desktop app checks stay as written. Other apps are not promised in v1. | Decided 2026-11-18 |
| D-168 | Desktop meeting app A added to the capture spike (owner request, 2026-11-18). The day-1 spike M0b-05 also checks that the Core Audio process tap captures the audio of desktop meeting app A and nothing else, because a desktop app can play audio from helper processes. The size stays M and the task stays on day 1. The owner installs the app and makes sure a test meeting can be started with a free account before day 1. The app is named only in the private Sprint Plan. The M2 check for desktop meeting app A stays as written (D-167). | Decided 2026-11-18 |
| D-169 | Operations Plan dropped (owner ruling, 2026-11-18). The owner does not need it. It is no longer a gate, a start condition or a reference: the step "accept the security gate" before day 1 is gone, checkpoint CK-1 passes without it, and its release criteria, SLOs, rollback and maintenance sections are not required for v1. What stays: the Threat Model and its controls X-13 to X-32 in the Task Breakdown (X-13 to X-16 still block v1.0, D-137), docs/guardrails.md, docs/definition-of-done.md, and the dependency audit (scripts/check.sh supply, deny.toml). Earlier mentions of the Operations Plan in decisions and tasks are history; the page stays private and is not maintained. The release criteria of D-111 now come from docs/definition-of-done.md and the Threat Model controls. | Decided 2026-11-18 |
| D-173 | Threat Model rewritten against PRD v33 (SW16-Q1). It is version 2 and lists 46 threats; every "Unconfirmed" item of version 1 is closed by a fact, a decision or a named control. This round changes documents only; no product code is written. |
| D-174 | X-16 is split in two (SW16-Q2). Part (a), https only and the redirect rule (D-175), is part of the done checks of M1-04 (day 3) and M1-42 (day 11), because the model downloader exists from day 3. Part (b), the CI check that no crate outside the policy opens a socket, with a rogue-crate canary, stays in X-16 (day 25). X-16 stays a v1.0 blocker (D-137). |
| D-176 | Driver installer gate (SW16-Q4). The X-15 checks are part of the done check of M1-45. The installer is not run on the owner's Mac until the X-15 tests pass: a file swapped after the hash check is refused, a file with the wrong signer is refused, and the administrator-prompt counter stays 0 in both cases. |
| D-177 | Residual risk rule (SW16-Q5). The Threat Model keeps the column "Residual risk" (today) and adds "Residual after control" (planned). A rating goes down only when its control is done with evidence. The four High risks (T-02, T-03, T-05, T-37) stay High until X-13, X-14, X-15 and X-16 are done with evidence (D-137). |
| D-178 | Hardware key in hand before day 7 (SW16-Q6). H-20 (hardware second factor on the release account) and H-21 (updater key in a hardware-backed store) need a hardware key. The owner has one before day 7 (27 Nov 2026). |
| D-183 | X-23 scope. Decided 2026-11-19 (proposed the same day, confirmed by the owner). CI already runs cargo deny (advisories, licences, sources) and the UI audit (jobs `supply-rust` and `supply-npm` in `.gitlab-ci.yml`). X-23 keeps only: builds with the locked files on a clean checkout, and the new-dependency line with a licence registry diff in the merge request template. |
| D-186 | Free-disk limit rule. Decided 2026-11-19 (proposed the same day, confirmed by the owner). The free-disk limit for Start is set after M2-22 (the 30-minute run) from the measured growth of journal and logs. Until then no number exists, and the disk budget item of X-22 stays open on purpose. |
| D-187 | Code of conduct removed (owner request, 2026-11-19; the owner merged MR !7). `CODE_OF_CONDUCT.md` no longer exists. This supersedes the code-of-conduct part of D-57. PRD section 21 drops the line; the ADR 0014 index line follows. |
| D-189 | Plan shift (owner, 2026-11-20). Day 1 passed with no code. Every date moves one working day later: day n takes the date that day n+1 had. One of the four spare days is used and the end of the window stays 20 Jan 2027. Task order, sizes and sprint numbers by day do not change. |
| D-190 | Spike code and notes (owner, 2026-11-20). Spike and measurement code lives in `bench/<task-id>/`, one crate per task with its own `[workspace]`, and is merged with its note `docs/m0/<task-id>.md`. The note of M0b-05 is `docs/m0/M0b-05.md`. Product code never imports from `bench/`. |
| D-191 | UI framework (owner, 2026-11-20). React stays. The Svelte comparison of Q57 is dropped for v1 because no task builds or measures a Svelte version. M0b-03 measures the React shell only. |

Other rows that these decisions touch:

| ID | Note |
|---|---|
| D-07 | Summary sections are Gist, Decisions, Open questions and Next steps. Key points are dropped from v1 and move to P7. |
| D-10 | The virtual microphone driver is never bundled. The install flow is superseded by D-69 (one-press install). |
| D-20 | Home and Today: see D-67. |

## Engine and audio

| ID | Decision |
|---|---|
| D-14 | ONNX Runtime in process by default. A helper process only if the M0-19 memo shows a model cannot load in process. |
| D-44 | Start checks in two layers. Layer 1, readiness, one rule for every mode: `canStart` = microphone, meeting-audio capture (process tap or the 2ch fallback device), virtual microphone and the four models all ok. Headphones never block readiness. Layer 2, confirmed at every Start: headphones (FR-72), meeting app microphone (FR-69), meeting app chosen (FR-68), routing. A failed check returns `START_CHECK_FAILED` with `details`. See ADR 0009. |
| D-45 | Quick actions have no second key. The result is pasted only after Enter at the preview; there is no "replace without preview" setting (ADR 0010). | Decided 2026-11-17 |
| D-49 | If the listening check M0-07 shows that whole English sentences are not good enough, work stops and the owner decides again. No fallback voice is prepared now (D-30 stays). |
| D-52 | Model download is confirmed by a native dialog shown by the Rust shell. The shell resolves the model id against a compiled-in table, never takes a URL from the webview, and refuses the download unless the licence was accepted. See ADR 0006; work item M1-04. |
| D-60 | Model manager: each URL and SHA-256 pinned in a manifest in `crates/model-store`; models come from their own public host; Interlo runs no server; resume, free-disk check; files in `~/Library/Application Support/Interlo/models`; progress event `models://progress`; `ModelInfo` has `licence` and `nonCommercial`; `download_model` takes `licenceAccepted`; new command `cancel_model_download`. See ADR 0012. |
| D-184 | Translation stage and spoken instructions. Decided 2026-11-19 (proposed the same day, confirmed by the owner). Five planted-instruction sentences are added to the M0-06 reading list. The result (the model obeys, prints the vocabulary list, or ignores them) is recorded in `docs/m0/`. |
| D-185 | Ask injection test at M3-19. Decided 2026-11-19 (proposed the same day, confirmed by the owner). The done check of M3-19 adds a planted-instruction transcript: no planted instruction shows in the answer and every citation is an existing line id. X-18 later puts the summary, Ask and quick actions on one shared prompt builder. |
| D-15 | Capture order: process tap for the chosen meeting app, then the 2ch virtual device for that app, else the session does not start. |
| FR-60 to FR-64 | One playback queue; segments play in order, once, never cut or skipped; backlog speeds up to 1.15x with pitch kept; the latency gate includes queue wait. |
| FR-65, FR-63 | Merge pauses under 600 ms without a sentence-final mark; cut at 10 s at the next pause. |
| FR-66, FR-67 | Engine failure in a segment: retry once, then keep the caption and play nothing. Unplug: hold the queue; after 30 s mark the rest "not played". |
| FR-70 | Owner English speech is detected from the recognizer output and not translated. |
| FR-72 | Built-in speakers are refused unless headphones are confirmed. |

## Data and privacy

| ID | Decision |
|---|---|
| G6 | Nothing leaves the Mac during a meeting. |
| A-13 | No local HTTP server. |
| Storage | SQLite with WAL, append-only journal, text only, cascade delete per meeting (the journal rows go through `Journal::delete_session`, ADR 0008). |
| D-46 | Deleting a meeting deletes its utterances, speakers, summaries, insights, jobs, journal rows and the notes created in it. Daily notes not tied to a meeting stay. Confirmed tasks stay with the meeting link cleared; unconfirmed drafts from that meeting are deleted. Task rule confirmed (D-53, H-19). See ADR 0008. |
| D-51 | No recorded human voice goes into the repository. Fixtures are synthetic (made from the sentences in the reading list, `docs/m0/reading-list.md`) plus live re-runs. Recordings used only for scoring (H-05) are deleted after scoring. M2-04 and M4-09 use synthetic audio or a live run. |
| D-53 | Task rule of D-46 is confirmed by the owner (H-19): confirmed tasks stay when a meeting is deleted, with the link cleared; unconfirmed drafts are deleted. |
| D-59 | Database and time rules. Schema v1 may change freely until the first public tag; after it every schema change needs a migration. `Store::conn()` becomes crate-private before E1 (E1-01). The journal sets `PRAGMA user_version` 1 and refuses a higher version. Times are stored as UTC milliseconds and shown and exported in the user's local time zone at the moment of use. See ADR 0013. |
| D-179 | Model integrity at every load. Decided 2026-11-19 (proposed the same day, confirmed by the owner). The engine host re-checks the SHA-256 of a model file before each load and refuses a symlink or a file not owned by the current user (control X-17, folded in M1-42). The PRD states the check at download only (FR-75): a PRD gap. |
| D-180 | Driver signer rule. Decided 2026-11-19 (proposed the same day, confirmed by the owner). If the driver publisher signs the installer, X-15 pins the signer next to the hash. If it does not, X-15 pins the hash only and the owner reads the diff of every driver bump, because no second reviewer exists (D-146). The fact is checked in M0-18. |
| D-181 | Crash output. Decided 2026-11-19 (proposed the same day, confirmed by the owner). A panic hook prints no caption text, and the app asserts at start that the core dump limit is 0 (control X-21). |
| D-182 | Hardened runtime. Decided 2026-11-19 (proposed the same day, confirmed by the owner). The release build has the hardened runtime on and no entitlement that disables library validation. X-14 checks both with `codesign`. |
| Logs | Timings and codes only; transcript text only when debug logs are on. |

## Hotkeys

Every default Interlo key uses Control plus Option (D-55): mute both directions
Ctrl+Option+M, show or hide the subtitle bar Ctrl+Option+S, quick translate
Ctrl+Option+T, fix grammar Ctrl+Option+G, save note Ctrl+Option+N. The app
shortcuts are Cmd+K (command bar) and Cmd+period (Stop, with a confirmation
dialog). The start-meeting key is removed. Each hotkey has an on/off switch in
Settings > General (M4); rebinding is E2 (FR-58). The E2 keys (translate,
grammar, save note) are unchanged. A single modifier with a letter is not
used, because it types a character on some keyboard layouts. A macOS system key is refused with
an error. A clash with another Interlo key warns. In the quick-action preview,
Enter pastes and Esc cancels (D-45).
