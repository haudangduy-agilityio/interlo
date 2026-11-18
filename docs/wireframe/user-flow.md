# Wireframe review: user flows and UI states

Source: Interlo wireframe artifact `MgLQKCFQgTfwWRf8ZV7gFT`, version 21.
Updated 2026-11-17 for D-61 to D-70, and 2026-11-18 for D-116 (Finishing state in J2), D-117, D-118 and D-122 (journeys J14 and J15). The wireframe is the final UI spec (D-62,
ADR 0017); where this file differs from it, this file changes.
Method: code reading of the wireframe script. No browser run yet, so every
state below is what the code does, not what a screenshot shows. The check is
not finished until the states are seen in a browser (see open questions).

## Journey summary

| ID | Journey | Trigger | Goal |
|---|---|---|---|
| J1 | Start a meeting | Start meeting (Home, Live meeting) | Two-way interpreting is running |
| J2 | Stop and save | Stop meeting | Meeting saved, summary written |
| J3 | Review summary and action items | Open a meeting | Accept or reject items, export |
| J4 | Find a past meeting | Meetings | Open, search, delete |
| J5 | Set up | Setup, or the Start gate | All required items ready |
| J6 | Add a vocabulary term | Settings > Vocabulary | Term saved for recognition |
| J7 | Change settings | Settings | Options saved |
| J8 | Quick translate, grammar, save note | Hotkeys in any app | Not in this wireframe (see gaps) |
| J9 | Pause and resume | Pause (Live meeting, compact window) | Translation held, then continued |
| J10 | Change the language pair | SPEAKS / HEARS button or swap button | Direction switched (v1: Vietnamese and English only) |
| J11 | Run a command | Cmd+K or "Search or run a command" | Screen or action reached without the mouse |
| J12 | Update the app | App start, every 24 hours, or "Check for updates" | New version installed after "Restart to update" |
| J13 | Install the virtual microphone | Install at Setup | Device ready after one press |
| J14 | Daily workspace | Home (Today section), notes, tasks, search, export | A note or task saved, found and exported |
| J15 | Failure recovery | The engine stops, or the headphones or the microphone are unplugged, in a meeting | The direction is back, or the user knows what to press |

## J1: Start a meeting (step table)

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| S-01 | user | Home or Live meeting (no meeting running) | Press Start meeting (always enabled; the menu next to it offers Both directions, Listen only, Speak only) | S-02 | |
| S-02 | system | Readiness gate (layer 1) | Check microphone, meeting audio, virtual microphone and models (four names) | Yes: S-02b, No: S-04 | |
| S-02b | system | Per-meeting checklist (layer 2) | Confirm headphones, meeting app microphone, meeting app and routing | Yes: S-03, No: S-06 | |
| S-03 | system | Live meeting, mode from the switch | Start capture and captions | S-05 | |
| S-04 | system | Start refused: Setup screen opens | Show a toast naming what is not on (`SETUP_INCOMPLETE`) | S-06 | |
| S-05 | user | Live meeting running | Talk, mute, pause (J9), change mode, change language pair (J10), stop | J2 | |
| S-06 | user | Setup screen | Fix an item, or press Re-check | S-01 | |

Decision D-1 (S-02, D-44): question "Is setup ready (microphone, meeting audio,
virtual microphone, models)?". Yes: S-02b. No: S-04 (Start refused, Setup opens).
Headphones are not part of this question; they are confirmed at S-02b. Decision D-1b (S-02b):
question "Are the per-meeting checks confirmed?". Yes: S-03. No: S-06 (the
checklist shows each failed item as a fix step). All exits exist.

State table (J1):

| Step | State | What the user sees | Next action for the user | Next step |
|---|---|---|---|---|
| S-03 | Loading models | Nothing. Live screen appears at once, captions stay empty | None | Gap: no loading state |
| S-04 | Not ready | Setup opens with the toast "Finish setup first. Not turned on yet: Virtual microphone, Models." (names in the order Microphone, Meeting audio, Virtual microphone, Models). It hides after 2.8 s | Fix the items shown on Setup | S-06 |
| S-03 | Permission denied (microphone) | Not handled. Setup shows "Microphone: Ready" fixed in the markup | None | Gap |
| S-03 | Paused | Button label becomes Resume; toast "Paused. Captions and audio are held." (see J9) | Resume | S-05 |

## J2: Stop and save

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| S-07 | user | Live meeting | Press Stop meeting | S-08 | |
| S-08 | system | Dialog "Stop the meeting and save it?" | Wait for choice | Yes (Stop and save): S-08b, No (Keep running): S-05 | |
| S-08b | system | Finishing... | Finish the sentence in progress, captions and speech included; wait at most 5 s (starting value, D-116, FR-71); then stop both directions and save the transcript | S-09 | |
| S-09 | system | Summary generating | Write summary locally | S-10 | |
| S-10 | system | Summary ready | Show summary, items, transcript | J3 | |

Decision D-2 (S-08): "Stop and save?". Yes: S-08b (Finishing...), then S-09. No: S-05. Both exits exist.

State table (J2):

| Step | State | What the user sees | Next action for the user | Next step |
|---|---|---|---|---|
| S-08b | Finishing | The state "Finishing..." (not in the wireframe yet; the wireframe stops at once). The sentence in progress is captioned and spoken. After 5 s without a sentence end, the sentence is cut and the meeting stops | Wait | S-09 |
| S-09 | Generating | Spinner and three steps, "about 38 s" | Wait | S-10 |
| S-09 | Failed | Not in the wireframe | None | Gap |
| S-10 | Empty meeting | Gist says "too short to summarize"; transcript empty state exists | Open another meeting | Meetings |

## J9: Pause and resume (D-65)

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| P-01 | user | Live meeting running (or compact window) | Press Pause | P-02 | |
| P-02 | system | Live meeting, paused | Stop translation in both directions; partners hear silence; the clock stops; toast "Paused. Captions and audio are held." | P-03 | |
| P-03 | user | Live meeting, paused | Decide: Resume or Stop? Resume: P-04. Stop: S-07 | | |
| P-04 | system | Live meeting running | Continue translation and the clock; toast "Resumed." | S-05 | |

Decision D-3 (P-03): "Resume or Stop?". Resume: P-04. Stop: J2 (Stop works while paused). Both exits exist.

## J10: Change the language pair (D-63)

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| L-01 | user | Top bar (SPEAKS / HEARS) | Press the swap button, or the pair button | Swap: L-03, Pair button: L-02 | |
| L-02 | system | Dialog "Language pair" | Show "Speaking X, hearing Y. v1 supports English and Vietnamese only." with Swap and Done | Swap: L-03, Done: L-04 | |
| L-03 | system | Top bar | Swap SPEAKS and HEARS; both chains reverse direction | L-04 | |
| L-04 | user | Same screen | Continue | S-05 or Home | Dialog closed |

The pair set is Vietnamese and English only; there is no other choice to pick.
The command bar item "Change language pair" opens L-02 (J11).

## J11: Run a command (command bar)

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| C-01 | user | Any screen | Press Cmd+K, or the sidebar button "Search or run a command" (Cmd+K again toggles it closed) | C-02 | |
| C-02 | user | Command bar, placeholder "Type a command or a meeting name" | Type to filter; Enter runs the first item; click runs another item; Esc closes | Item run: C-03, Esc: C-04 | |
| C-03 | system | Target screen or action | Go to Home, Live meeting, Meetings or Setup; start or stop a meeting; open Settings; mute; partner voice; subtitle bar; compact window; light or dark appearance; change language pair; "Open <meeting title>" | | Command done |
| C-04 | system | Previous screen | Close the bar | | Closed |

State: no match shows the hint "No match. Try "start" or "settings"." Stop from the command bar goes to the stop confirmation (S-08).

## J12: Update check (D-61, ADR 0015)

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| U-01 | system | App start, every 24 hours, or the user presses "Check for updates" (Settings footer or About) | Is a meeting running? Yes: U-02. No: U-03 | | |
| U-02 | system | Meeting running | Do nothing; check again after the meeting | | Deferred |
| U-03 | system | Background | Request the manifest file; newer version? Yes: U-04. No: U-07 | | |
| U-04 | system | Background | Is "Update automatically" on? Yes: U-05. No: U-08 | | |
| U-05 | system | Background | Download, verify the signature. Valid: U-06. Wrong: U-09 | | |
| U-06 | user | Notice "Restart to update" | Press it when no meeting runs; the app never restarts during a meeting | | Updated |
| U-07 | system | After a manual check | Say that this is the latest version (the wireframe toast reads "You have the latest wireframe build.") | | Up to date |
| U-08 | system | Notice | Tell that a new version exists; download nothing | | Informed |
| U-09 | system | Notice | Refuse the update; keep the current version | | Refused |

Decisions: D-4 (U-01) "Is a meeting running?"; D-5 (U-03) "Is there a newer version?"; D-6 (U-04) "Is Update automatically on?"; D-7 (U-05) "Is the signature valid?". Every question has a Yes and a No exit. The wireframe shows only the toast of U-07; the other states are specified by D-61 and need a screen before M4.

## J13: Install the virtual microphone (D-69, ADR 0016)

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| I-01 | user | Setup, row "Virtual microphone (16 channels)" | Press Install | I-02 | |
| I-02 | system | Native confirmation (name, source, size) | Confirm? Yes: I-03. No: stop | | Cancelled |
| I-03 | system | Installing | Download from the pinned URL; check SHA-256. Match: I-04. Mismatch: I-08 | | |
| I-04 | system | macOS administrator prompt | Run the installer; user enters the password in the macOS prompt (the app never sees it). Accepted: I-05. Cancelled: I-08 | | |
| I-05 | system | Waiting | Wait up to 15 s for the device. Appears: I-06. Not: I-08 | | |
| I-06 | system | Loopback test | Play the test phrase, run the setup check again | I-07 | |
| I-07 | system | Setup | Row shows ready; if macOS needs an audio service restart or a restart of the Mac, say so in plain words | | Ready |
| I-08 | system | Setup, row failed | Show a plain message naming the step that failed; Install can be pressed again | I-01 | |

The user runs no Terminal steps. The wireframe simulates one press of Install; the
failure states are specified by D-69 and need screens.

## J14: Daily workspace (D-67, D-122)

Planned in E1 and E2 (tasks E1-01 to E1-06, E2-03). The wireframe has none of these screens; this table is the source for them.

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| W-01 | user | Home | Open the app | W-02 | |
| W-02 | system | Home, Today section above Recent meetings | Show today's tasks, the notes made today and today's meetings; meeting action items appear as drafts (FR-45, FR-48). Is there anything to show? Yes: W-03. No: W-03 with an empty Today section | | |
| W-03 | user | Home | Choose: quick note (W-04), new task (W-06), search (W-08) or export (W-10). The key Ctrl+Option+N saves selected text as a note from any app (FR-50, E2-03) and joins this journey at W-04 | W-04, W-06, W-08 or W-10 | |
| W-04 | user | Quick note | Write the note and choose one tag (a client or a project) (FR-46). Save? Yes: W-05. No (cancel): W-03, nothing is saved | | |
| W-05 | system | Today section | The note shows at once in Today; it keeps its time and tag after a restart (FR-45, FR-46) | W-03 | |
| W-06 | user | New task | Enter a title and a due date (FR-47). Save? Yes: W-07. No (cancel): W-03, nothing is saved | | |
| W-07 | system | Today section | The task shows with the status open; an overdue task is marked; a draft from a meeting becomes a task only after a click (FR-47, FR-48) | W-03 | |
| W-08 | user | Search | Type keywords; filter by tag and by day (FR-49) | W-09 | |
| W-09 | system | Search results | Any match? Yes: show the notes and tasks, the user opens one. No: show the state "No match" and keep the filters | Yes: W-03, No: W-08 | |
| W-10 | user | Export | Select notes and tasks to export | W-11 | |
| W-11 | system | Markdown file | Write the selected items as Markdown in the same format as meeting notes (FR-52); the file opens in a Markdown viewer | | Exported |

Decisions: D-8 (W-02) "Is there anything for today?"; D-9 (W-04, W-06) "Save or cancel?"; D-10 (W-09) "Is there a match?". Every question has a Yes and a No exit. The empty Today text, the "No match" text and the cancel dialog are not specified in the wireframe and need screens before E1.

## J15: Failure recovery (D-117, D-118, D-122)

Three failures during a running meeting: the engine stops, the headphones are unplugged, the microphone is unplugged. Banner texts come from FR-35, FR-36, DEV-02 and DEV-04; "my language" and "partner language" stay correct after Swap (D-99).

| Step | Actor | Screen or state | Action | Next | Exit |
|---|---|---|---|---|---|
| R-01 | system | Live meeting running (S-05) | Detect a failure. Which one? Incoming engine stopped: R-02. Outgoing engine stopped: R-04. Headphones unplugged: R-06. Microphone unplugged: R-09 | | |
| R-02 | system | Live meeting, red banner | Incoming chain stopped: the partner's original audio plays at full volume and captions keep running. Banner: "Translation to <my language> stopped. You are hearing the partner's original audio. Captions keep running in <partner language>." with the button "Retry translation" (FR-35). The engine is restarted within 3 s (FR-11) | R-03 | |
| R-03 | user | Live meeting, red banner | Wait, or press "Retry translation". Did translation return? Yes: R-14. No: R-02 stays, and the original audio keeps playing | | |
| R-04 | system | Live meeting, red banner | Outgoing chain stopped: a chime plays in the headphones and the meeting hears silence. Banner: "Your <partner language> voice stopped. The meeting hears silence until you decide." with the buttons "Retry voice" and "Send my real microphone to the meeting". Nothing switches on its own (FR-36) | R-05 | |
| R-05 | user | Live meeting, red banner | Choose. "Retry voice": R-14 if the voice returns, else R-04. "Send my real microphone to the meeting": the banner reads "The meeting hears your real microphone, so partners hear <my language>." with the button "Return to translated voice"; pressing it returns to the translated voice: R-14 | R-14 or R-04 | |
| R-06 | system | Live meeting, red banner (DEV-02) | Headphones unplugged: only the Vietnamese voice stops, the outgoing direction keeps running; queued audio is held (FR-12, FR-67). Banner: "Headphones disconnected. Your outgoing direction keeps running. The Vietnamese voice is stopped." with the button "Retry output" | R-07 | |
| R-07 | system | Live meeting | Is the device plugged in again within 30 s? Yes: R-08. No: R-08b | | |
| R-08 | system | Live meeting | The queued audio plays automatically; no button is needed (FR-67, D-118) | R-14 | |
| R-08b | user | Live meeting, banner stays | The rest of the queue is marked not played in the captions (FR-67). Plug the device in again, or press "Retry output", the manual fallback that also covers a device the app does not recognise (FR-12, D-118). Output back? Yes: R-14. No: R-06 | | |
| R-09 | system | Live meeting, red banner (DEV-04) | Microphone unplugged: the outgoing direction stops. Banner: "Microphone disconnected. Your outgoing direction is stopped. Reconnect it or pick another microphone." with the button "Retry microphone" (D-117, FR-12) | R-10 | |
| R-10 | user | Live meeting, red banner | Reconnect the microphone or pick another one, then press "Retry microphone". Did the outgoing direction start? Yes: R-14. No: R-09 | | |
| R-14 | system | Live meeting running | The failed direction runs again; a gap is marked in the captions where text was lost (FR-11) | S-05 | Recovered |

Decisions: D-11 (R-01) "Which failure?" (four exits); D-12 (R-03) "Did translation return?"; D-13 (R-07) "Is the device back within 30 s?"; D-14 (R-08b) "Is the output back?"; D-15 (R-10) "Did the outgoing direction start?". Every question has a Yes and a No exit. The wireframe has the banners for incoming, outgoing and headphones; the DEV-04 banner, the "Retry microphone" button and the "Finishing..." state (J2) are new and need screens.

## Exits

| Journey | Exit | What is kept |
|---|---|---|
| J1 | Stop and save | Transcript, notes, summary (per the stop dialog) |
| J1 | Keep running | Everything continues |
| J1 | Window closed mid-meeting | Not in the wireframe. Must be defined |
| J2 | Summary fails | Not in the wireframe. Transcript is saved (per the dialog text), summary state unknown |
| J2 | Sentence does not end within 5 s | The sentence is cut, the meeting stops and is saved (D-116) |
| J9 | Stop while paused | Same as J2 |
| J15 | Failure not recovered | The failed direction stays stopped with its banner and button; the other direction keeps running; nothing switches on its own (FR-35, FR-36, DEV-04) |
| J12 | Update refused or deferred | The current version keeps running; nothing is installed |

## Gaps by severity

### P1: blocks a user or breaks a promise

1. **Quick actions and daily workspace are missing.** Decision D-43 puts E1 (notes, tasks, search, export) and E2 (quick translate, grammar, save note) in v1. The wireframe has none of their flows. Only the word "Today" and "Tasks" appear as text. J14 now specifies the daily workspace flow (D-122); the screens are still to draw.
2. **Cloud fallback is still in the wireframe.** Settings row "Cloud engine as last fallback", its dialog, and the cloud actions still exist. D-42 removes cloud from v1.
3. **No loading state when the models load on Start.** D-A8 says models load on Start. The live screen appears with no wait message.
4. **No permission-denied state.** Microphone permission is hard-coded "Ready". A denied permission is not shown anywhere. Resolved in the rule by D-44 (microphone is a layer 1 item named "Microphone"); the wireframe screen is still to change.
5. **Summary failure has no state.** Status chips cover ready, generating, and "Interrupted" only.
6. **Edit an action item: wrong state.** `item-save` shows "Saved" even when the new text is empty, and nothing changes. It also edits the English text while the view may show Vietnamese.

### P2: confusing, but a user can recover

7. **Setup "Finish" has no reason when disabled.** The button is disabled until all eight items are ready, so the "Some items are missing" toast can never show. The Start gate needs only five items. Home says "Ready when you are" for the five, while Finish asks for eight. Resolved in the rule by D-44: one readiness rule, `canStart`, for Home, Setup and Start; the wireframe screens are still to change.
8. **Installing the virtual microphone (16 channels) is simulated.** The flow has no admin-password step, no failure, and no "restart the Mac" step. Owner check H-02 says a restart is needed. J13 now specifies the flow (D-69); the wireframe screens for the failure states are still to draw.
9. **Meetings: one message for two cases.** "No meetings match" shows both when the library is empty and when a search matches nothing. There is no button to clear the filter, though the text says "show all meetings".
10. **Interrupted meeting has no view.** The filter and chip exist, but nothing says what was saved or how to finish it.
11. **Paused has no persistent state.** Only the button label changes and a 2.8 s toast shows. J9 specifies Pause (D-65); the Live timer chip shows a paused marker.
12. **Reject has no undo.** The toast says "left out of export", nothing else.
13. **Toasts carry the only feedback** for saves, retries, and failures. They disappear after 2.8 s. Keep the failures in the page, not in a toast.

## States not covered (by screen)

| Screen | Loading | Empty | Error | Partial | Done |
|---|---|---|---|---|---|
| Home | Not needed | Recent meetings: table header only, no sentence | Not covered | Ready card lists 3 groups | Covered |
| Setup | Installing: covered | n/a | Install failed: missing | Some items ready: covered | Finish: disabled without reason |
| Live meeting | Missing (model load) | Covered | Banners: covered for in, out, headphones | Mode off: covered | Stop: covered |
| Subtitle bar | n/a | Covered ("Waiting for the partner") | Missing | n/a | n/a |
| Summary | Covered | Covered | Missing | Covered (unsupported items) | Covered |
| Meetings | n/a | Wrong message for empty library | n/a | Search empty: no clear button | Covered |
| Detail: transcript | n/a | Covered | n/a | n/a | n/a |
| Detail: action items | n/a | Covered | n/a | n/a | Edit wrong (see gap 6) |
| Vocabulary | n/a | Not checked | n/a | n/a | Term added: covered |
| Quick actions and E1 | Not in wireframe | Not in wireframe | Not in wireframe | Not in wireframe | Not in wireframe |

## Diagram

Not drawn yet. The user-flow skill asks for a flowchart next to this table. This
pass covers J1 and J2 in tables only.

## Open questions

- [U] Owner (H-02): does the virtual microphone (16 channels) need a Mac restart before it works? The setup flow must show it if yes.
- Closed by D-61 to D-74: Vietnamese interface (D-70), updates (D-61), driver install (D-69), pause (D-65, D-72), language pair (D-63, D-71), mute both (D-73), network wording (D-74).
- [U] Owner: should the wireframe show E1 and E2 now, since D-43 puts them in v1?
- [U] Owner: is the cloud row kept as a removed item, or deleted from the wireframe?
- [U] Owner: how should a summary failure look to the user (retry only, or keep the transcript and offer export)?
- Closed by D-116, D-117, D-118 and D-122: the "Finishing..." state at Stop (J2), the unplugged microphone (DEV-04, J15), the reconnect of the headphones (J15), and the journeys J14 and J15. The screens for them are still to draw.
- Closed by D-46: deleting a meeting deletes the notes created in it (J4 delete); confirmed tasks stay with the link cleared.
- [U] Owner: after a window close mid-meeting, is the meeting saved as "Interrupted" (the wireframe assumes so)?

## Next steps

1. Fix P1 items 2 and 6 in the wireframe now (small edits).
2. Add states for items 3, 4, 5 and the missing flows in item 1 before the next review.
3. Run the browser check: force each failure with the demo buttons, then take a screenshot of each state.
