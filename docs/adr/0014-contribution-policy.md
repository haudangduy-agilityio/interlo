# 0014. Contribution policy: DCO sign-off, no CLA, protected main

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-57

## Context

Interlo is open source under the MIT licence (ADR 0003). The project needs a
light way to confirm that a contributor may submit a change, and a shared rule
for what reaches the main branch.

## Decision

- Contributors sign off every commit with the Developer Certificate of Origin
  (`git commit -s`, a `Signed-off-by:` line). There is no CLA.
- A CI job named `dco` fails a pull request that has a commit without a
  `Signed-off-by:` trailer.
- The main branch is protected: the required CI check and at least one review.
  The owner sets this after the repository exists (H-20).

## Consequences

- No contributor signs a legal agreement; the sign-off line is the proof.
- A commit made without `-s` must be amended before the pull request can merge.
- Branch protection is a setting on the hosting site. It is not in the
  repository, so it needs an owner action (H-20).
- Task X-12 covers the files and the DCO check.
- Amended 2026-11-19 (D-187): the owner removed the code of conduct file and
  its references. This policy no longer includes a code of conduct.
