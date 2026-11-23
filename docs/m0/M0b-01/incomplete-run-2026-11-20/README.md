# An incomplete run (not part of the evidence)

`03-sockets-launch1.log` is the socket log of one run of `scripts/check.sh app` on commit `ee2a351` that stopped at sample 51
of launch 1: from that sample on the pid list holds only the app pid, so the app and its web view helper processes had ended
(sample 50 still lists all four pids). The cause was not found: there is no crash report in `~/Library/Logs/DiagnosticReports`,
and the Mac was in use. The step failed and wrote no pass line; the run was repeated from the start and its result is in
`evidence/`. The file is kept so the failed run is not hidden. It is not covered by `evidence/SHA256SUMS`.
