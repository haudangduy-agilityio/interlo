# Interlo

Local macOS app for live two-way interpreting in meetings. The owner speaks
Vietnamese and the meeting hears English; partners speak English and the owner
hears Vietnamese in headphones. It also writes captions and a post-call summary,
and has a daily workspace (notes, tasks, quick translate). Everything runs on the
Mac. Rust core (Tauri 2) plus a React UI. MIT licence.

Read only the files your task needs. Put a new rule in the file that owns its
topic, not here.

| When you | Read |
|---|---|
| Change anything | `docs/guardrails.md` |
| Start or continue a task (spec, plan, code, verify, review, MR, release) | `docs/workflow.md` |
| Decide where code goes | `docs/folder-structure.md` |
| Need what a crate or screen owns, and must never do | `docs/architecture.md` |
| Write code | `docs/coding-style.md`, `docs/naming-conventions.md` |
| Add or upgrade a dependency | `docs/tech-stack.md` |
| Change a command or an event between Rust and the UI | `docs/ipc-contract.md` |
| Set up, build or run | `docs/development.md` |
| Write tests | `docs/testing.md` |
| Finish a change | `docs/definition-of-done.md` |
| Commit | `docs/git.md` |
| Need why a rule exists (decision IDs, planning pages) | `docs/decisions.md` |
| Make a new architecture decision | write one file in `docs/adr/`, then add its line to `docs/decisions.md` |
| Need the machine baseline or model candidates | `docs/m0/` |
| Work on UI flows or UI states | `docs/wireframe/user-flow.md` |
| Build, change or review any screen, component, style or UI text | `docs/design.md`, then `docs/reading-the-design-system.md` (read the design system files and cite them) |
