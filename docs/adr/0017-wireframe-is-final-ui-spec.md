# 0017. The wireframe is the final UI spec

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-62

## Context

The PRD, the task breakdown and the roadmap were written before the wireframe
reached its present form. They differ from it in many small places. Two
sources of truth for the same screen cost review time and produce code that
matches neither.

## Decision

The wireframe is the final UI spec. Where the PRD, the task breakdown or the
roadmap differ from it, they change to match the wireframe, except where a
later decision says otherwise. Wireframe demo helpers are not product: the
"Demo:" buttons and toasts, the scripted call and the sample data.

Decisions taken in the same round:

- D-63: language pair control (SPEAKS / HEARS, swap); v1 supports Vietnamese and English only, direction switchable.
- D-64: Off means no translation, no captions and no voice; the meeting is still saved.
- D-65: Pause stops translation in both directions and stops the clock; Stop works while paused.
- D-66: the Play button on a transcript line re-speaks stored text; no audio is stored.
- D-67: Home as in the wireframe in M1 to M4; E1 adds a "Today" section.
- D-68: two generic meeting app choices at Start (in the browser, desktop).
- D-70: interface language English and Vietnamese; owner voices have male and female presets with a gender tag.

D-61 (updates) and D-69 (driver installer) have their own ADRs, 0015 and 0016.

## Consequences

- A reviewer checks a UI change against the wireframe first. A difference is
  fixed in the code or recorded as a new decision, not left open.
- `docs/wireframe/user-flow.md` follows the wireframe, not the other way round.
- The wireframe strings are the approved text for disclosures and banners.
- Code for these decisions is not part of this decision. Each one has a task
  in the task breakdown.
