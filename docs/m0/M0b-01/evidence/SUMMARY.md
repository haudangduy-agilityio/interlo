# Evidence summary

Commit measured: `d685bf22c023497c4cb97c2886c2bb489d78809c`. Date: 2026-11-23T14:19:07+0700. Command: `scripts/check.sh app`.
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
  > RESULT: title "Interlo", size 1200 by 780 points, found 1054 ms after launch (limit 60000 ms); two screenshots of window 2164
- `02-interrupted-windows.log`:
  > none (no such file in this run)
- `03-sockets-launch1.log`:
  > RESULT: 60 samples in 60 s (0 late by more than 1 s), pids 65288,65301,65302,65303, sockets seen: 0, no wildcard listener and no non-loopback address
- `03-sockets-launch2.log`:
  > RESULT: 60 samples in 60 s (0 late by more than 1 s), pids 68411,68423,68424,68425, sockets seen: 0, no wildcard listener and no non-loopback address
- `03-sockets-launch3.log`:
  > RESULT: 60 samples in 60 s (0 late by more than 1 s), pids 72085,72097,72098,72099, sockets seen: 0, no wildcard listener and no non-loopback address
- `03-interrupted-launches.log`:
  > none (no such file in this run)
- `04-rss-launch1.txt`:
  > RESULT: launch 1, 5 samples: 97760 97872 97984 98000 98000 median 97984 KB, min 97760 KB, max 98000 KB
- `04-rss-launch2.txt`:
  > RESULT: launch 2, 5 samples: 84752 84752 84752 84752 84752 median 84752 KB, min 84752 KB, max 84752 KB
- `04-rss-launch3.txt`:
  > RESULT: launch 3, 5 samples: 79792 79936 79936 79936 79936 median 79936 KB, min 79792 KB, max 79936 KB
- `04-rss-all-launches.txt`:
  > RESULT: 15 samples over 3 launches: median 84752 KB, min 79792 KB, max 98000 KB, spread 18208 KB
- `05-negative-controls.txt`:
  > RESULT: 22 controls, 0 wrong
- `06-checksum-control.txt`:
  > RESULT: the changed file was refused
- `07-telex-typing.log`:
  > NOT RUN (the typing check is opt-in: scripts/check.sh app --typing)

## Screenshots (stored locally, not in git)

- `9ec153d834dd27d83870c919b7c2574c6b5e4c4293b8a788a14705f6e5a81df6  /Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M0b-01/first-frame-win2164-t1054ms-epoch1795418481.png`
- `5acd92f2dddaddce3adb2d6513d346333411186867dd88708d7340f4c6b23508  /Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M0b-01/after-5s-win2164-t6215ms-epoch1795418486.png`
- `a970843796fc09a46e0044c70e34d61bc589a63bb11bda1ea54e32568afb6ea6  /Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M0b-01/home.png`
- `dc97dc052ad5f2f1a27bbe0756f831551bea75605bd9acfb7a71138517b7af77  /Users/saccerwin/Desktop/workspace/training/ai/interlo-evidence/M0b-01/setup.png`

`home.png` and `setup.png` were retaken on the new UI after the run, by pressing the sidebar buttons through the Accessibility API (no key events) and capturing the window by its id; they are not produced by the script.

## Negative controls (05-negative-controls.txt)

- CONTROL 1: override guard with CARGO_PROFILE_RELEASE_OPT_LEVEL=1 in the environment  -> behaved as expected (refuse, exit 1)
- CONTROL 2: override guard in a clean environment  -> behaved as expected (accept, exit 0)
- CONTROL 3: rustc flags, the real bin line  -> behaved as expected (accept, exit 0)
- CONTROL 4: rustc flags with -C opt-level=1  -> behaved as expected (refuse, exit 1)
- CONTROL 5: rustc flags with -C codegen-units=16  -> behaved as expected (refuse, exit 1)
- CONTROL 6: rustc flags with -C lto=off  -> behaved as expected (refuse, exit 1)
- CONTROL 7: rustc flags without a strip flag  -> behaved as expected (refuse, exit 1)
- CONTROL 8: rustc flags with -C panic=abort  -> behaved as expected (refuse, exit 1)
- CONTROL 9: socket rule on a test listener bound to 127.0.0.1:56915 (lsof: n127.0.0.1:56915 )  -> behaved as expected (accept, exit 0)
- CONTROL 10: socket rule on a test listener bound to 0.0.0.0:56916 (lsof: n*:56916 )  -> behaved as expected (refuse, exit 1)
- CONTROL 11: socket rule on a test listener bound to [::]:56917 (lsof: n*:56917 )  -> behaved as expected (refuse, exit 1)
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
- interrupted launches: none.
- The screenshots show the window at two moments (the first poll that found it, and 5 s later); they are not a recording, and they are kept outside git.
- Rerun `scripts/check.sh app` to compare: the pass lines must repeat, the memory numbers vary (read the median and the spread).
