# Naming conventions

## General

- English only. One term for one concept, used the same way in code, UI and docs.
- Never use the name of another product or brand. Use generic terms.
- Fixed terms: "meeting", "owner" (the person using the app), "partner" (a remote
  speaker), "direction" (`out` = owner to meeting, `in` = meeting to owner),
  "line" (one caption line), "segment" (one unit of audio or text in the chain),
  "virtual microphone" (the 16ch virtual device that carries the owner's English
  voice into the meeting).

## Rust

| Item | Rule | Example |
|---|---|---|
| Crate | kebab-case noun for what it owns | `audio-io`, `model-store`, `workspace-notes` |
| Module and file | snake_case | `language_gate.rs` |
| Type and trait | UpperCamelCase; traits name a role | `CaptureMethod`, `Translator` |
| Function | snake_case verb phrase | `capture_plan`, `check_hotkey` |
| Constant | SCREAMING_SNAKE_CASE with the unit | `MERGE_PAUSE_MS`, `MAX_SPEED_FACTOR` |
| Error enum | `<Crate>Error` or `<Area>Error` with thiserror | `ModelError`, `CaptureError` |
| Not built yet | return `Err(NotImplemented)`, never `todo!()` | |
| Time | unix milliseconds as `u64`, field names end in `_ms` | `created_ms` |

## IPC (Tauri)

| Item | Rule | Example |
|---|---|---|
| Command | snake_case verb then object | `start_meeting`, `list_models` |
| JSON fields | camelCase (`#[serde(rename_all = "camelCase")]`) | `canStart`, `missingGroups` |
| Error code | SCREAMING_SNAKE_CASE | `SETUP_INCOMPLETE` |
| Event | `<area>://<name>` | `meeting://caption` |

The contract is in [ipc-contract.md](ipc-contract.md). Change the contract file,
the Rust DTOs and `apps/ui/src/ipc/types.ts` in one change.

## TypeScript and React

| Item | Rule | Example |
|---|---|---|
| Feature folder | kebab-case under `src/features/` | `meeting-detail/` |
| Component file | UpperCamelCase `.tsx` | `MeetingsScreen.tsx` |
| Other file | camelCase `.ts` | `mockTransport.ts` |
| Component | UpperCamelCase function component | `EmptyState` |
| Hook | `use` + UpperCamelCase | `useSetupStatus` |

## UI text

- Short sentences, one action per button label ("Start meeting", "Open Setup").
- Error text says what failed and what to do next.
- Group names in setup messages: "Virtual microphone", "Models".

Test names are in `docs/testing.md`.
