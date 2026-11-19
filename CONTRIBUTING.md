# Contributing

Thank you for helping. Keep changes small and tested.

1. Read [CLAUDE.md](CLAUDE.md). It routes you to the one doc that owns each
   topic. Read [docs/guardrails.md](docs/guardrails.md) before any change.
2. Read [docs/workflow.md](docs/workflow.md). A task moves from SPEC to PLAN to
   tests first, then verify, review and merge request.
3. Make the change inside the crate or feature that owns it
   ([docs/folder-structure.md](docs/folder-structure.md)). Write the test for
   each rule before the code ([docs/testing.md](docs/testing.md)).
4. Run the gate before you open a merge request, and paste its output in the
   description:

   ```bash
   npm --prefix apps/ui ci
   scripts/check.sh full
   ```

5. Commit as described in [docs/git.md](docs/git.md): English, imperative, with
   the task ID when there is one, for example
   `M1-08: add playback to the virtual microphone`.
6. Sign off every commit (`git commit -s`). A commit without `Signed-off-by`
   fails the DCO check of the GitHub workflow (D-57); the GitLab pipeline does not check it,
   so the owner checks it in review. The sign-off says you have the right to submit
   the change under the project licence. There is no CLA.
7. The `main` branch is protected. A change merges only with the required CI
   check passing and at least one review.
8. A change under `.claude/`, `scripts/`, a CI file, `apps/ui/eslint.config.js` or
   the Tauri capabilities and build files runs code on a maintainer's machine. The
   maintainer reviews it by hand before running an agent on that branch (see
   `.gitlab/CODEOWNERS`).

Do not add a dependency, a crate or a file outside the agreed scope without
asking first. Never commit models, audio files or databases.

Report a security problem through [SECURITY.md](SECURITY.md), not in an issue.
