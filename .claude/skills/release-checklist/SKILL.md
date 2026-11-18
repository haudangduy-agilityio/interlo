---
name: release-checklist
description: Checklist for cutting an Interlo release: version sync, gate, build, Developer ID signing, notarization, stapling, DMG smoke test, CHANGELOG, and the tag. Use when the owner asks whether the merged work is ready to ship or names a release. Reads and reports; it never tags or pushes by itself.
disable-model-invocation: true
argument-hint: "[version, e.g. 0.2.0]"
allowed-tools: Bash(git status *), Bash(git branch --list *), Bash(git branch --show-current), Bash(git diff *), Bash(git log *), Bash(git tag --list *), Bash(git ls-files *), Bash(git rev-parse *), Bash(scripts/check.sh *), Bash(grep -H *), Bash(jq *), Bash(codesign --verify *), Bash(codesign -dv *), Bash(spctl --assess *), Bash(xcrun stapler validate *), Read, Grep, Glob
---

# Release checklist

Version to ship: $ARGUMENTS (empty: ask; never guess it)

Station 10 of `docs/workflow.md`, decision D-40 (`docs/adr/0004-developer-id-signing-and-notarization.md`):
release builds are signed with a Developer ID certificate and notarized, on the owner's
Mac. CI builds and tests only and holds no signing secret. There is no automatic update.

This skill reads and reports. It does not tag, push, sign or upload on its own. Signing and
notarization use the owner's keychain, and the owner types or approves them. The tag is
created only after the owner names it.

## 1. Merged and clean

- On the default branch, at the merge commit of the work to ship; `git status --short` is
  empty. Every MR in the release has acceptance (`ACCEPTANCE.md`) with each AC ACCEPTED.
- No task in the release is BLOCKED or DRAFT. Open owner decisions are listed, not hidden.

## 2. Version sync

One version in all three places, equal to the version above:

| File | Field |
|---|---|
| `Cargo.toml` and each crate's `Cargo.toml` (root has none; check `crates/*/Cargo.toml` and `apps/desktop/src-tauri/Cargo.toml`) | `version` |
| `apps/desktop/src-tauri/tauri.conf.json` | `version` |
| `apps/ui/package.json` (and the top of `apps/ui/package-lock.json`) | `version` |

```bash
grep -H '^version' crates/*/Cargo.toml apps/desktop/src-tauri/Cargo.toml
jq -r '.version' apps/desktop/src-tauri/tauri.conf.json apps/ui/package.json
```

Any mismatch is a failure. `Cargo.lock` is updated by the build; commit it with the bump.
The bump is its own commit that the owner approves.

## 3. Gate

```bash
scripts/check.sh full
```

Exit 0 is required. Exit 2 (NOT CONFIGURED) is BLOCKED. End-to-end journeys are outside the
gate; run `/verify-app` and report them as a separate line, never folded into the gate result.

## 4. Build

Build the release bundle on the owner's Mac from a clean checkout of that commit:
`npm --prefix apps/ui run build`, then the Tauri release build. `bundle.active` is `false`
in `apps/desktop/src-tauri/tauri.conf.json` today. Report that as a blocker for the owner to
decide; do not change the config inside this checklist. Record the exact build command that
was used.

## 5. Sign, notarize, staple (owner's machine)

Identities and profiles come from the owner's keychain. Never write a certificate, a password
or an API key into a file, a command line you save, the repository, a log or CI.

```bash
codesign --force --options runtime --timestamp --sign "$DEVELOPER_ID_APPLICATION" "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"
xcrun notarytool submit "$ZIP_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$APP_PATH"
xcrun stapler validate "$APP_PATH"
spctl --assess --type execute --verbose "$APP_PATH"
```

Set `DEVELOPER_ID_APPLICATION`, `NOTARY_PROFILE`, `APP_PATH` and `ZIP_PATH` in the shell
first; ask the owner for any value you do not have. Notarization must report `Accepted`. A
rejected log is attached to the report. The entitlements for microphone and audio capture
come from the bundle config; check them with `codesign -d --entitlements - "$APP_PATH"`.

## 6. DMG and smoke test

Package the stapled app into a DMG, sign the DMG, notarize and staple it the same way. Then
smoke test the DMG on a Mac that has not built it, or a fresh user account:

- Mount the DMG, copy the app out, open it. No Gatekeeper warning.
- `spctl --assess --type open --context context:primary-signature -v "$DMG_PATH"` accepts it.
- The app starts, shows the Setup or Home screen, and asks for microphone permission under
  the name Interlo.
- The app reports its version and it equals the version being shipped.
- No outbound connection while idle (`lsof -nP -i -a -p <pid>` shows none beyond loopback).
  Nothing leaves the Mac without a model download that the owner starts (guardrail 1).
- No model, audio file or database sits inside the bundle (D-39; `docs/folder-structure.md`).

A step you could not run is "not run", with the reason. It is never a pass.

## 7. CHANGELOG and notes

`CHANGELOG.md` at the repository root has an entry for the version: date, what changed for
the owner, known gaps. List only merged work, by task ID. If the file does not exist, say so
and propose its first entry; create it only when the owner agrees. Release notes use the
same text.

## 8. Tag

Run `git tag --list` and show the latest tags. Do not create or push a tag until the owner
names it in chat (for example `v0.2.0`), and push only to the remote and branch the owner
names. State the tag, the commit it points at, the remote and the target in one line before
you do it.

## Report

Verdict first: ready, not ready, or ready with named exceptions. Then a table: item, pass /
fail / not run, evidence. Paste the output of every failure. Never say ready while a check is
red or not run without a stated reason. If ready, ask the owner for the tag name.
