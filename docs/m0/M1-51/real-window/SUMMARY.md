# M1-51 real-window evidence

Recorded on 2026-11-20 on the owner's Mac (Mac mini, macOS 26.4.1). Images are not in git (owner rule): they are in `/Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M1-51/`, and `SHA256SUMS` lists each one with its hash. This folder holds only text.

Builds: the screens of part A were captured from the signed local bundle built at `012818c`; the narrow-window "after" captures from the bundle built at `47f38fa`. The "before" narrow captures are from the bundle built at `e25b641` plus the fonts (before the narrow fix). The real Rust shell has no `list_notes`, `list_meetings` or `check_setup` yet, so Home, Meetings, Workspace and Setup show their error states with Retry; no mock data is in this set.

## A. Every screen in the real window (done)

Driver: `capture_screens.py`; log: `actions.log` (each click with its ref, each capture with the window id). The window was read through `agent-desktop snapshot --window-id`, never by app name. Screens, in dark and light: Home (`home-*`), Live meeting (`live-*`), Meetings (`meetings-*`), Workspace Today and Notes (`workspace-today-*`, `workspace-notes-*`), Setup (`setup-*`), Settings sheet (`settings-*`), Start pressed (`start-pressed-*`, a toast: the shell cannot answer the setup check). The light capture switched the macOS appearance for about 30 s and restored it: `actions.log` has `appearance switched to light` and `appearance restored dark=True`. Meeting detail was not captured: the shell returns no meetings, so there is nothing to open.

Real finding: the Mode select on Live showed as a thin bar in this window (owner screenshot). It is fixed in `012818c`; `live-dark.png` and `live-light.png` show the fixed select.

## B. Narrow window (done, one defect found and fixed)

Scripts: `narrow_window.py`. Logs: `narrow-before-fix.log`, `narrow-after-fix.log`, `narrow-negative-control.log`. `tauri.conf.json` sets `"width": 1200, "height": 780` and no minimum width, and the shell accepted every size asked: `narrow-after-fix.log` has `resize asked=300x780 ok=True applied=300.0x780.0`. So the shell has no minimum; this is for the owner to decide (nothing was changed in `tauri.conf.json`).

Defect before the fix (`before-fix-*.png`): below 761 px the sidebar became a row that scrolled sideways and clipped its entries, Settings and the Local-only card were hidden (`display: none`), and at 480 and 300 px the top bar ran out of the window. The demo's own rule at 760 px (a row sidebar with sideways scroll) has the same clipping; the fix does not copy it: the sidebar wraps (brand and search, navigation, Local-only card and Settings) and the top bar wraps. The 980 px rule (one column of cards) was already in the app.

Check in the real window after the fix: `narrow-after-fix.log` has `interactive_elements_right_of_window=[]` for each of 18 rows (widths 900, 800, 760, 640, 480, 300 times Home, Live, Workspace), measured from the accessibility bounds. Negative control: `narrow-negative-control.log`, the same check with the window pretended 250 px narrower, lists `'Search or run a command ⌘K', 'Live meeting', 'Meetings', 'Settings', 'Start meeting', ...` at 480 px, so the check does detect controls outside the window. Test that fails before the fix: `apps/ui/e2e/layout.spec.ts`; the old CSS gives `negative-control-layout-before-fix.txt` (`✘ ... at 760 px`, `✘ ... at 480 px`, `✘ ... at 360 px`, `Error: sidebar scrolls sideways`, `Error: page scrolls sideways`), the new CSS passes all six.

Limit: the bounds check sees controls that leave the window to the right only; it does not see overlap or text clipped inside a control. Overlap and clipping were read from the screenshots by eye (`after-fix-*.png`).

## C. Vietnamese text in the real window (not established; BLOCKED)

The owner stopped input control (key events, input-source change, clipboard) during this part, so it stopped here and the scripts that post key events and change the input source are not committed (they are in the local folder, `tools-not-committed/`).

What exists:
- Run 1 (Telex, field empty before): the field read back equal to the expected text, NFC, 244 characters, 0 missing and 0 extra code points. Its log and screenshot were overwritten by the next runs, so there is no kept evidence of it. Do not count it as PASS.
- Run 2 (`telex-run-2-us-layout-incomplete.log`) and run 3 (`telex-run-3-incomplete.log`): the field held 70 and 65 of 244 characters (`field length=70 expected_length=244 equal_nfc=False`, `field length=65 ... equal_nfc=False`): the start of the typed text was lost. The cause was not found (the field was empty before each run and the app was frontmost while typing). Run 2 meant to be a negative control with a plain layout, but the source did not take effect cleanly, so it is not a clean control either. The comparison itself is controlled: `control tampered_expected_equal=False (must be False)` in run 3.
- Fallback font (no key events needed): of the 122 code points typed, the bundled text font holds 30 and 92 need a fallback (`gap-summary.txt`: `typed=122 held_by_text_font=30 fallback_needed=92`). Core Text says the system UI font holds all 92: `fallback-font.log`: `system UI font: .SFNS-Regular family: .AppleSystemUIFont`, `held by the system UI font: 92 ... no glyph anywhere: []`. So no missing-glyph box is expected. In the screenshot of run 3 (`telex-note-field.png`, 65 characters) no missing-glyph box is visible; that image is a partial field, so it is weak evidence, and the Core Text result is the stronger one.

## Not verified

- Vietnamese typed into the note field with Telex, end to end (part C): BLOCKED as above.
- Meeting detail in the real window (no meetings from the shell).
- Overlap and clipped text in the narrow window, other than by eye.
- The capture of dark and light was made with this Mac's settings (dark by default); other displays and scale factors were not tried.
