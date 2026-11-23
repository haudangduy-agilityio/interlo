# Evidence summary

Commit measured: `132db79ef10f4ed3d5ca7b18b6ff2f737902deea`. Date: 2026-11-23T10:23:30+0700. Command: `scripts/check.sh app`.
The raw files in this folder are the evidence; this page only quotes them. `SHA256SUMS` lists every other file and the hash of each screenshot with its local path.
Screenshots are not in the repository: they are stored in `/Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M0b-01`.
Verify the text files: `cd docs/m0/M0b-01/evidence && grep -v ' /' SHA256SUMS | shasum -a 256 -c`. The lines that start with an absolute path are the screenshots: they verify only on the machine that holds them.

## Quoted result lines

- `00-bundle.txt`:
  > exit: 0
  > RESULT: signed with "Interlo Local Signing"; codesign --verify --deep --strict exit 0
- `01-build-flags.txt`:
  > FLAGS lib: -C opt-level=3 -C linker-plugin-lto -C codegen-units=1 -C strip=symbols 
  > FLAGS bin: -C opt-level=3 -C lto=thin -C codegen-units=1 -C strip=symbols 
  > RESULT: 2 rustc commands (lib bin) have opt-level=3, codegen-units=1, a strip flag, no panic flag; the bin has lto=thin; no profile or rustflags override found
- `02-window-poll.log`:
  > RESULT: title "Interlo", size 1200 by 780 points, found 755 ms after launch (limit 60000 ms); two screenshots of window 1157
- `02-interrupted-windows.log`:
  > none (no such file in this run)
- `03-sockets-launch1.log`:
  > RESULT: 60 samples in 60 s (0 late by more than 1 s), pids 68365,68379,68380,68381, sockets seen: 0, no wildcard listener and no non-loopback address
- `03-sockets-launch2.log`:
  > RESULT: 60 samples in 60 s (0 late by more than 1 s), pids 72275,72304,72305,72306, sockets seen: 0, no wildcard listener and no non-loopback address
- `03-sockets-launch3.log`:
  > RESULT: 60 samples in 60 s (0 late by more than 1 s), pids 77113,77145,77146,77147, sockets seen: 0, no wildcard listener and no non-loopback address
- `03-interrupted-launches.log`:
  > RESULT: launch 3, try 1: the app process ended before the readings were complete (2026-11-23T10:27:37+0700); the launch was repeated; partial logs: 03-sockets-launch3-try1-interrupted.log
- `04-rss-launch1.txt`:
  > RESULT: launch 1, 5 samples: 87520 87520 87520 87520 87520 median 87520 KB, min 87520 KB, max 87520 KB
- `04-rss-launch2.txt`:
  > RESULT: launch 2, 5 samples: 71408 72096 72080 72480 72432 median 72096 KB, min 71408 KB, max 72480 KB
- `04-rss-launch3.txt`:
  > RESULT: launch 3, 5 samples: 70480 70048 68704 68336 67216 median 68704 KB, min 67216 KB, max 70480 KB
- `04-rss-all-launches.txt`:
  > RESULT: 15 samples over 3 launches: median 72096 KB, min 67216 KB, max 87520 KB, spread 20304 KB
- `05-negative-controls.txt`:
  > RESULT: 22 controls, 0 wrong
- `06-checksum-control.txt`:
  > RESULT: the changed file was refused
- `07-telex-typing.log`:
  > NOT RUN (the typing check is opt-in: scripts/check.sh app --typing)

## Screenshots (stored locally, not in git)

- `f52b901cef9e674c44ba2d62700340561416f51b6d6c9969193eb0a77581d049  /Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M0b-01/first-frame-win1157-t755ms-epoch1795404318.png`
- `3102defbad126068a1a5a207688d80cb7cb5e785551263ff984891ba0ad5e704  /Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M0b-01/after-5s-win1157-t5950ms-epoch1795404323.png`

## Negative controls (05-negative-controls.txt)

- CONTROL 1: override guard with CARGO_PROFILE_RELEASE_OPT_LEVEL=1 in the environment  -> behaved as expected (refuse, exit 1)
- CONTROL 2: override guard in a clean environment  -> behaved as expected (accept, exit 0)
- CONTROL 3: rustc flags, the real bin line  -> behaved as expected (accept, exit 0)
- CONTROL 4: rustc flags with -C opt-level=1  -> behaved as expected (refuse, exit 1)
- CONTROL 5: rustc flags with -C codegen-units=16  -> behaved as expected (refuse, exit 1)
- CONTROL 6: rustc flags with -C lto=off  -> behaved as expected (refuse, exit 1)
- CONTROL 7: rustc flags without a strip flag  -> behaved as expected (refuse, exit 1)
- CONTROL 8: rustc flags with -C panic=abort  -> behaved as expected (refuse, exit 1)
- CONTROL 9: socket rule on a test listener bound to 127.0.0.1:57448 (lsof: n127.0.0.1:57448 )  -> behaved as expected (accept, exit 0)
- CONTROL 10: socket rule on a test listener bound to 0.0.0.0:57449 (lsof: n*:57449 )  -> behaved as expected (refuse, exit 1)
- CONTROL 11: socket rule on a test listener bound to [::]:57450 (lsof: n*:57450 )  -> behaved as expected (refuse, exit 1)
- CONTROL 12: window rule: Interlo, 1200 by 780, 1324 ms  -> behaved as expected (accept, exit 0)
- CONTROL 13: window rule: size 1000 by 700  -> behaved as expected (refuse, exit 1)
- CONTROL 14: window rule: title Other  -> behaved as expected (refuse, exit 1)
- CONTROL 15: window rule: 61000 ms  -> behaved as expected (refuse, exit 1)
- CONTROL 16: window rule: empty title is BLOCKED (exit 3), not a pass  -> behaved as expected (refuse:3, exit 3)
- CONTROL 17: memory rule: 5 readings  -> behaved as expected (accept, exit 0)
- CONTROL 18: memory rule: 3 readings  -> behaved as expected (refuse, exit 1)
- CONTROL 19: memory rule: 3 numbers and 2 broken lines  -> behaved as expected (refuse, exit 1)
- CONTROL 20: text-only rule: a folder with one text log  -> behaved as expected (accept, exit 0)
- CONTROL 21: text-only rule: a folder with a PNG image  -> behaved as expected (refuse, exit 1)
- CONTROL 22: prerequisites with a signing identity that does not exist (BLOCKED is exit 3, caused by the identity)  -> behaved as expected (refuse:3, exit 3)

## Limits

- covered by the socket check: the app process, its children and the WebKit helper processes that appeared after the launch.
- not attributable: a helper process that exists before the launch (it serves another app); a helper that another app starts during the run (counted here, which only makes the check stricter).
- not seen: a connection that opens and closes between two 1 s samples; DNS lookups done for the app by a system daemon; network work done for the app by another process; Unix-domain sockets (lsof -i lists internet sockets only).
- scope: the app idle on its first screen for 60 s, three launches. This is evidence about the app process tree and its web view helpers while idle: not a proof that nothing leaves the Mac, and not a reading during a meeting.
- a helper process that another app starts during the run is counted too: if it has a non-loopback socket the check fails and its address is written to the log, so close other apps that use a web view while this runs.
- memory covers the app process only, not the helper processes.
- interrupted launches: 1 launch(es) ended by themselves and were repeated (see 03-interrupted-launches.log); the cause was not found.
- The screenshots show the window at two moments (the first poll that found it, and 5 s later); they are not a recording, and they are kept outside git.
- Rerun `scripts/check.sh app` to compare: the pass lines must repeat, the memory numbers vary (read the median and the spread).
