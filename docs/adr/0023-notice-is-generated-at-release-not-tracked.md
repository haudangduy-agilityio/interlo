# 0023. NOTICE is not tracked; it is generated at release and shipped in the app

- Status: accepted
- Date: 2026-11-23
- Decision ID: D-192
- Relates to: ADR 0003 (D-39), ADR 0011 (D-47), D-56

## Context

The repository kept a `NOTICE` file with third-party licence attribution. It named
libraries and models, and D-56 allowed those names only in `docs/tech-stack.md` and
`NOTICE`. The owner decided on 2026-11-23 that the repository does not track the file.
The notice is a release artifact: it is built from the dependency records and ships
with the app, which is where a user who has the binary can read it.

## Decision

- `NOTICE` is not tracked. `/NOTICE` is in `.gitignore`, so a local or generated copy
  never enters a commit by accident.
- The release build generates the notice from the records the repository keeps:
  `docs/tech-stack.md` (libraries and models) and the private licence record
  `docs/m0/candidates.md`. It puts the notice inside the app bundle.
- The repository text no longer says that a `NOTICE` file is in the repository and no
  longer links to it. Where it names the notice, it says the notice is generated at
  release and shipped inside the app.
- Guardrail 3 and D-56 change the place where libraries and models may be named:
  `docs/tech-stack.md`, the generated notice, and the licence texts that sit beside
  bundled font files. Every other repository text still names only Interlo.
- When fonts are bundled (the UI work in M1-51 and X-50), their licence texts go beside
  the font files in the repository, so a font's licence travels with the font. No
  font file is tracked yet.
- The model manager's licence column keeps the text "See NOTICE". The string is shown
  by an app that contains the notice (see the follow-up below), so it stays true once
  the release build does what this ADR requires.

## Consequences

- Earlier commits keep the old `NOTICE` file. This ADR does not rewrite history.
- The public source tree has no attribution file. A reader of the repository finds the
  licence records only in `docs/tech-stack.md` and the private record
  `docs/m0/candidates.md`; a user of a release finds the full notice in the app.
- A release without the generated notice would ship binaries with no attribution and
  would make "See NOTICE" in the model list false. The release must fail, not warn.
- `scripts/check-templates.mjs` no longer treats `NOTICE` as a repository path, so an
  issue or merge request template that names it is not checked against the disk.
- `docs/m0/candidates.md` stays where it is, and it still names the studied models.
  Whether a tracked file that names models fits the "no other product names" rule is an
  open question for the owner; this ADR does not decide it.

## Follow-up for the release tasks (M4-15, X-10, X-09)

These tasks are not changed by this ADR. When they are built, they must cover:

1. A release step generates `NOTICE` from `docs/tech-stack.md` and
   `docs/m0/candidates.md`, and the DMG build puts it in the app bundle's resources.
2. A release check opens the built bundle and fails when the notice is missing or
   empty, or when a model or library in `docs/tech-stack.md` has no entry in it.
3. The About screen or the model licence screen can open the bundled notice, so the
   text "See NOTICE" has a target in the app.
4. The release notes tell the owner that the generated file is not committed.
