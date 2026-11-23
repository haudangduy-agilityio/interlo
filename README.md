# Interlo

Interlo is a local macOS app for live two-way interpreting in meetings. You
speak Vietnamese and the meeting hears English. Partners speak English and you
hear Vietnamese in your headphones. Interlo also writes live captions and a
post-call summary, and has a daily workspace with notes, tasks and quick
translate.

Everything runs on your Mac. The core is written in Rust (Tauri 2) and the
interface in React.

## Status

**Pre-alpha, skeleton.** The workspace, the IPC contract and the UI shell
exist. Live interpreting does not work yet. Do not use Interlo in a real
meeting.

## Platform

- macOS 14.4 or later (Core Audio process tap needs 14.4).
- Apple silicon only in v1.
- Windows and Linux come after v1.

## Install

There is no release yet. When there is one, you get a DMG file from the owner or from the
GitHub Releases of this repository. Interlo is not in an app store and Apple has not
checked it, so macOS warns you the first time you open it. Expect this.

1. Open the DMG and drag Interlo to Applications.
2. Open Interlo. macOS says it cannot check the app. Click Done.
3. Open System Settings, then Privacy & Security. Scroll to the Security section and click
   "Open Anyway" next to Interlo. Enter your password if macOS asks.
4. Open Interlo again and click Open. After that it opens like any other app.
5. Allow the microphone when Interlo asks.

Check the file before you open it: `shasum -a 256 Interlo.dmg` must print the checksum
that comes with the release. These steps may differ a little on other macOS versions.

## Build from source

You need Rust 1.99.1 (see `rust-toolchain.toml`; it includes the `clippy` and
`rustfmt` components) and Node 26 with npm. See `docs/development.md` for the full setup.

```bash
npm --prefix apps/ui ci --ignore-scripts   # the same install CI runs
scripts/check.sh quick   # format, lint, unit and integration tests, typecheck, GitLab template check
scripts/check.sh full    # quick, then UI build, desktop build (the gate)
scripts/check.sh e2e     # end-to-end journeys, out of the gate for now
scripts/check.sh supply  # dependency audit; needs cargo-deny; runs weekly in CI, not in the gate
```

`scripts/check.sh` is the gate on your Mac. It stops at the first failing step and
never reports a pass when a tool or a script is missing.

Run `scripts/check.sh full` and `scripts/check.sh e2e` on your Mac before you open a
merge request, and paste their real output in the description (section Validation).
On GitLab, `.gitlab-ci.yml` runs only lint and tests, in two parallel jobs: `rust`
(format, clippy, tests) and `ui` (lint, format, typecheck, tests). The jobs have no runner
tag, so any runner that takes untagged jobs runs them; on Linux the Tauri shell crate is
left out. A weekly dependency audit (`supply-npm`, `supply-rust`) runs only from a pipeline
schedule. It does not build the app, run the end-to-end journeys or check the sign-off.
`.github/workflows/ci.yml` runs the full gate on a macOS runner after the move to GitHub
(D-112). Neither runs until a runner exists for it: the GitLab pipeline is off until the
CI/CD variable `INTERLO_CI` is `true`.

To run the app window after a UI build:

```bash
npm --prefix apps/ui run build
cargo run -p interlo-desktop
```

## Privacy promise

- Nothing leaves your Mac during a meeting. There is no cloud engine, no
  account and no backend in v1.
- The only network use is a model download or a driver installer download that
  you start with a click, and the update check outside meetings. Every outbound
  call goes through one policy gate.
- Audio is never stored. Only text is saved.
- Models are not bundled and are checked against a checksum before they run.
  Models are downloaded by you after you accept each model's licence; some
  models are trained on non-commercial data (see the third-party notice inside the
  app).

## Reporting problems and updates

Nothing is sent automatically. To report a problem, use Settings > Export
diagnostics. It writes a redacted file (timings, event names and error codes;
no transcript text) and you attach it to an issue yourself. This feature is
planned (M3-09).

Interlo updates itself. It checks for a new version when it starts and every 24
hours, and never while a meeting is running. It downloads the update in the
background from the GitHub Releases of this repository, checks its signature
(the update key is its own key; Interlo is not signed by Apple), and then shows
"Restart to update". It never restarts during a meeting. Settings has a switch,
"Update automatically", on by default. With the switch off, Interlo only tells
you that a new version exists and downloads nothing. The check sends only the
request for the manifest file: no machine id, no user id and no usage data. The
server sees your IP address, as any web server does for a web request. This
feature is planned (M4).

You must follow the recording laws where you live.

## Licence

MIT. See [LICENSE](LICENSE). Models are not part of this repository and carry
their own licences; third-party notices are generated at release and shipped
inside the app.

## Documentation

Reference files are in [docs/](docs/). Start with
[docs/development.md](docs/development.md) to set up and
[docs/architecture.md](docs/architecture.md) to see what each crate owns.
Architecture decisions are recorded in [docs/adr/](docs/adr/). To contribute,
read [CONTRIBUTING.md](CONTRIBUTING.md). To report a vulnerability, read
[SECURITY.md](SECURITY.md).
