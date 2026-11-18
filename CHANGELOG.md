# Changelog

All notable changes to this project are recorded in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Rust workspace with the core crates, a Tauri 2 shell and a React UI shell.
- `scripts/check.sh` gate (`quick` and `full`) and a CI workflow that runs it.
- Architecture decision records in `docs/adr/` (0007 part 2 is proposed).
- `NOTICE` with third-party licence attribution.
- Start checks in two layers.
- Quick actions paste only after Enter.
- Deleting a meeting deletes notes created in it.
- Licence screen before model download (planned).
- DCO sign-off for every commit, a `dco` CI job and a code of conduct (Contributor Covenant 2.1).
- Model manager design: pinned URL and checksum manifest, resume, progress event (ADR 0012).
- Database and time rules: journal `user_version`, UTC milliseconds (ADR 0013).

- Automatic updates design: check outside meetings, signed with the updater's own key, from GitHub Releases, switch "Update automatically" (ADR 0015, planned M4).
- Driver installer design: one press at Setup, pinned URL and SHA-256, administrator prompt, loopback test (ADR 0016, planned M1).
- Command bar (Cmd+K) design (planned M4).
- Pause design: translation held in both directions, the clock stops (planned M2).
- Language pair design: swap between Vietnamese and English, both chains reverse; Swap is refused during a meeting (planned M2, D-71).
- Pause drops speech and writes a "Paused" marker line in the transcript (D-72). Mute both directions also sets the partner voice to the original English (D-73). The privacy text lists all three network uses (D-74).
- Wireframe sync: the wireframe is the final UI spec (ADR 0017); Home, navigation, Settings sheet, Stop dialog and summary sections follow it.

### Changed

- Default hotkey for mute my voice is Ctrl+Option+M (all defaults use Control plus Option). It now mutes both directions; the start-meeting key is removed and Ctrl+Option+S shows or hides the subtitle bar.
- Network rule: model download or driver installer download that the owner starts, and the update check outside meetings (guardrail 1).
- Summary sections are Gist, Decisions, Open questions and Next steps; Key points move to a later phase.
