# M0 measurement folder

- `baseline.md`: machine baseline (M0-01). Done 2026-11-16.
- `candidates.md`: candidate sheet and licence status (M0-02). Done 2026-11-16; licence reads recorded.

- `measurement.md`: the owner's procedure for the M1 latency gate (D-50).
- `M0b-01/evidence/`: raw text logs of `scripts/check.sh app` (build flags, window, sockets, memory, negative controls), `SUMMARY.md` and `SHA256SUMS`. Screenshots are not in git: they are stored locally in `../interlo-evidence/M0b-01/` and listed by hash in `SHA256SUMS`.
- `X-01.md`: `scripts/check.sh full` exits 0 locally and in CI on main (commit `c1821a9`), with both summary tables (2026-11-23).
- `M0-03.md`: the bench harness: one stage on one file, one session clock; code in `bench/M0-03/` (2026-11-23).
- `M0b-02.md`: release profile and release binary size (2026-11-20).
- `M0b-03.md`: signed bundle size, idle memory and sockets of the desktop shell (2026-11-20).
- `M0b-04.md`: Vietnamese input check in a text field of the shell; PASS (AC-9, AC-10), checked by hand by the owner on 2026-11-23, no log in the repository. The scripted typing check stays opt-in and was not run in this pull request. The log of the failed 2026-11-20 attempts is in `M0b-01/typing-attempts-2026-11-20/`.

Results from M0-03 onward go into this folder. No number goes into the PRD before its file exists here.
