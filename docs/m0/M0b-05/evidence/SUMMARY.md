# M0b-05 evidence summary

- Date (UTC): 2026-11-23T03:59:48Z
- Commit: ddc254a0f74b30c0f932b9c932a7b73993f4a884 (working tree files changed: 0)
- macOS: 26.4.1 on Mac16,11; rustc 1.99.1 (58cf8f9c3 2026-10-13)
- Spike binary SHA-256: 3db94c84faeda8a129db3a4c008a78d9635ed151533417576504bb7653df7ec3
- Every status below is read from a raw file in this folder; `SHA256SUMS` holds the hash of every file.
- PASS, NO, NOT RUN and BLOCKED are the SPEC status words. A check that could not run is never PASS.

| AC | Status | File | Deciding line |
|---|---|---|---|
| AC-11 | PASS | `01-ac11/result.txt` | AC-11 PASS exit=0 PASS idle_floor=-120.0 present_seconds=10/10 unmeasured_seconds=0 means_dbfs=[-18.1, -23.1, -18.5, -18.4, -17.3, -16.3, -14.1, -18.2, -16.2, -16.3]; quiet_windows=30/30 windows_outside_3dB=0 max_level=-120.0 |
| AC-12 | PASS | `02-ac12/result.txt` | AC-12 PASS PASS idle_tone=-120.0 mean_tone=-120.0 windows=50/50 limit_db=3 |
| AC-13 | PASS | `03-ac13/result.txt` | AC-13-browser PASS PASS idle_tone=-120.0 mean_tone=-120.0 windows=100/100 limit_db=3 |
| AC-14 | PASS | `04-ac14/result.txt` | AC-14 PASS attempt 1: pids that started an output stream after the trigger: 1 (see probe-1.log); PASS idle_floor=-120.0 mean_level=-120.0 max_window=-120.0 windows=100/100 |
| AC-15 | BLOCKED | none | needs desktop meeting app A with a second person (H-14): no account and no second person |
| AC-16 | PASS | `05-ac16/result.txt` | AC-16 PASS PASS SILENT 10013 ms after TAP_START (limit 11000 ms) |
| AC-17 | PASS | `11-ac17/result.txt` | AC-17 PASS PASS lines=["PERMISSION_REFUSED", "route=2ch"] expected=["PERMISSION_REFUSED", "route=2ch"] late_lines=0 capture_started=true exit_code=Some(0) |
| AC-18 | PASS | `06-ac18/result.txt` | AC-18 PASS PASS restart_after_rate_change_ms=181 first_present_window_end_ms_after_restart=1000 idle_floor=-120.0 |
| AC-19 | PASS | `08-ac19/result.txt` | AC-19 PASS substitute source (tone written into the 2-channel device): PASS route=Some("route=2ch") want=route=2ch no_capture_route=false; idle_floor=-120.0 present_seconds=10/10 unmeasured_seconds=0 means_dbfs=[-17.4, -17.0, -... |
| AC-20 | PASS | `09-ac20/result.txt` | AC-20 PASS PASS lines=["app=missing", "route=2ch"] expected=["app=missing", "route=2ch"] late_lines=0 capture_started=true exit_code=Some(0) |
| AC-35 | PASS | `07-ac35/result.txt` | AC-35 PASS PASS route=Some("route=tap") want=route=tap no_capture_route=false; idle_floor=-120.0 present_seconds=10/10 unmeasured_seconds=0 means_dbfs=[-18.6, -15.0, -16.4, -16.5, -15.3, -16.7, -14.9, -16.0, -16.8, -21.0] |
| AC-36 | PASS | `10-ac36-granted/result.txt` | AC-36-app-missing PASS PASS lines=["app=missing", "NO_CAPTURE_ROUTE 2ch=missing"] expected=["app=missing", "NO_CAPTURE_ROUTE 2ch=missing"] late_lines=0 capture_started=false exit=Some((2, 1795406737237)) exit_after_start_ms=Som... |
| AC-40 | PASS | `12-ac40/result.txt` | AC-40 PASS PASS lines=["PERMISSION_REFUSED", "app=missing", "route=2ch"] expected=["PERMISSION_REFUSED", "app=missing", "route=2ch"] late_lines=0 capture_started=true exit_code=Some(0) |
| AC-21 | PASS | `14-ac21/result.txt` | AC-21 PASS both find commands exit 0 and print nothing |
| AC-22 | PASS | `15-ac22/result.txt` | AC-22-gate PASS check.sh quick exit 0, last line 'ALL STEPS PASSED (quick)'; packages under bench/ in the root metadata: 0 |
| AC-39 | PASS | `16-ac39/result.txt` | AC-39 PASS 28 run logs, each with sample lines and no non-loopback address or wildcard listener (see per-run.txt) |

## Repeated timings (median and spread)

- AC-16-repeat PASS: SILENT after TAP_START in ms over 5 runs: n=5 median=10013 min=10006 max=10029 (limit 11000) (`17-ac16-repeat/result.txt`)
- AC-18-repeat PASS: over 5 runs: RESTART after RATE_CHANGE in ms: n=5 median=84 min=73 max=181 (limit 3000); first audio-present one-second window ends after RESTART in ms: n=5 median=1000 min=1000 max=2000 (limit 3000) (`18-ac18-repeat/result.txt`)

## Negative controls

A control runs the same check on input that must fail. CONTROL-OK means the check reported NO.

| AC | Control | File | Output |
|---|---|---|---|
| AC-11 | CONTROL-OK(no sound plays) | `01-ac11/result.txt` | NO idle_floor=-120.0 present_seconds=0/10 unmeasured_seconds=0 means_dbfs=[-120.0, -120.0, -120.0, -120.0, -120.0, -120.0, -120.0, -120.0, -120.0, -120.0]; quiet_windo... |
| AC-12 | CONTROL-OK(own process included in the tap) | `02-ac12/result.txt` | NO idle_tone=-120.0 mean_tone=-17.1 windows=49/50 limit_db=3 |
| AC-13 | CONTROL-OK(tone plays from the target) | `03-ac13/result.txt` | NO idle_tone=-120.0 mean_tone=-13.7 windows=99/100 limit_db=3 |
| AC-14 | CONTROL-OK(the target plays during the 10 s) | `04-ac14/result.txt` | NO idle_floor=-120.0 mean_level=-17.7 max_window=-12.0 windows=100/100 |
| AC-16 | CONTROL-OK(the target plays the whole run) | `05-ac16/result.txt` | NO no TAP_START or no SILENT line |
| AC-17 | CONTROL-OK(the check wants the no-cause route line) | `11-ac17/result.txt` | NO lines=["PERMISSION_REFUSED", "route=2ch"] expected=["route=tap"] late_lines=0 capture_started=true exit_code=Some(0) |
| AC-18 | CONTROL-OK(the rate does not change) | `06-ac18/result.txt` | NO no RATE_CHANGE or no RESTART line |
| AC-19 | CONTROL-OK(the check wants route=tap on a 2ch run) | `08-ac19/result.txt` | NO route=Some("route=2ch") want=route=tap no_capture_route=false; idle_floor=-120.0 present_seconds=10/10 unmeasured_seconds=0 means_dbfs=[-17.4, -17.0, -17.0, -17.0, ... |
| AC-19 | CONTROL-OK(nothing plays into the 2-channel device) | `08-ac19/result.txt` | NO route=Some("route=2ch") want=route=2ch no_capture_route=false; idle_floor=-120.0 present_seconds=0/10 unmeasured_seconds=0 means_dbfs=[-120.0, -120.0, -120.0, -120.... |
| AC-20 | CONTROL-OK(the target app is running) | `09-ac20/result.txt` | NO lines=["route=tap"] expected=["app=missing", "route=2ch"] late_lines=0 capture_started=true exit_code=Some(0) |
| AC-35 | CONTROL-OK(the check wants route=2ch on a tap run) | `07-ac35/result.txt` | NO route=Some("route=tap") want=route=2ch no_capture_route=false; idle_floor=-120.0 present_seconds=10/10 unmeasured_seconds=0 means_dbfs=[-18.6, -15.0, -16.4, -16.5, ... |
| AC-36 | CONTROL-OK(the check wants a different cause list) | `10-ac36-granted/result.txt` | NO lines=["app=missing", "NO_CAPTURE_ROUTE 2ch=missing"] expected=["PERMISSION_REFUSED", "app=missing", "NO_CAPTURE_ROUTE 2ch=missing"] late_lines=0 capture_started=fa... |
| AC-36 | CONTROL-OK(the check wants app=missing too) | `13-ac36-refused/result.txt` | NO lines=["PERMISSION_REFUSED", "NO_CAPTURE_ROUTE 2ch=missing"] expected=["PERMISSION_REFUSED", "app=missing", "NO_CAPTURE_ROUTE 2ch=missing"] late_lines=0 capture_sta... |
| AC-40 | CONTROL-OK(the check wants one cause only) | `12-ac40/result.txt` | NO lines=["PERMISSION_REFUSED", "app=missing", "route=2ch"] expected=["PERMISSION_REFUSED", "route=2ch"] late_lines=0 capture_started=true exit_code=Some(0) |
| AC-21 | CONTROL-OK(a decoy .wav file is found) | `14-ac21/result.txt` | /var/folders/x8/qxtlpplj6mv3j38pnp06r9qm0000gn/T/m0b-05/decoy.wav |
| AC-22 | CONTROL-OK(the bench manifest read directly lists a bench package) | `15-ac22/result.txt` | packages under bench/: 1 |
| AC-39 | CONTROL-OK(a wildcard listener is sampled) | `16-ac39/result.txt` | NO samples=1 violations=wildcard listener: m0b05 1 user 4u IPv4 0x1 0t0 TCP *:8080 (LISTEN) |

Controls that did not fail as they must: 0.

## Other lines

- PERMISSION-grant DONE: permission code after the prompt: 0 (0 granted, 1 refused); route lines: 1 (`91-permission-grant/result.txt`)
- PERMISSION-deny DONE: permission code after the prompt: 1 (0 granted, 1 refused); route lines: 1 (`92-permission-deny/result.txt`)
- PERMISSION-grant DONE: permission code after the prompt: 0 (0 granted, 1 refused); route lines: 1 (`93-permission-grant/result.txt`)
