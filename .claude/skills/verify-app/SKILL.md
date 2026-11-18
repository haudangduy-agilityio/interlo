---
name: verify-app
description: Drive the Interlo UI in a real browser against the mock transport, through the end-to-end suite (scripts/check.sh e2e) or one Playwright journey, and keep evidence under docs/agent-tasks/<ID>/evidence/. Use after a change to a screen, a UI state, the IPC client or a wireframe journey, or when asked to prove a screen works.
disable-model-invocation: true
argument-hint: "[task ID and journey, e.g. M1-08 start-meeting, or 'all']"
allowed-tools: Read, Grep, Glob, Bash(scripts/check.sh e2e), Bash(npm --prefix apps/ui run e2e), Bash(npm --prefix apps/ui run e2e *), Bash(npm --prefix apps/ui run build), Bash(lsof *), Bash(mkdir -p docs/agent-tasks/*), Bash(cp * docs/agent-tasks/*), Bash(git status *), Bash(git rev-parse *), Write(docs/agent-tasks/**)
---

# Verify the running UI

Task and journey: $ARGUMENTS

End-to-end journeys are outside the gate for now. `/verify` runs `scripts/check.sh full`;
this skill runs the journeys apart, so a screen change gets a real browser as well as unit
tests. Run both.

## What this run proves, and what it does not

The UI runs against `apps/ui/src/ipc/mockTransport.ts`. A pass proves the screens, their
states (loading, empty, error, done) and the calls they make through `src/ipc/client.ts`.
It does not prove the Rust core, the audio chain, a model, the Keychain or Tauri
capabilities. Write "UI against the mock transport" in the report, and never "verified in
the app".

## Preconditions (BLOCKED when any fails)

1. The journey exists in `apps/ui/e2e/<journey>.spec.ts` and traces to a flow in
   `docs/wireframe/user-flow.md`. Name that flow in the report.
2. `apps/ui/package.json` defines an `e2e` script, and Playwright and its browser are
   installed. `scripts/check.sh e2e` exits 2 when something is missing; that is BLOCKED,
   not a pass. Do not install packages or browsers to get past it; ask the owner.
3. The port the e2e config uses is free before you start:
   `lsof -nP -iTCP:<port> -sTCP:LISTEN` prints nothing. The Vite dev server defaults to
   5173 (`apps/ui/vite.config.ts`, `strictPort`). If something already listens, it is not
   yours: do not drive it and do not stop it by process name.
4. The browser address stays on localhost. The browser guard allows loopback with an
   explicit port and nothing else.

## Drive

Evidence goes under `docs/agent-tasks/<ID>/evidence/`. The folder is excluded from git.

```bash
mkdir -p docs/agent-tasks/<ID>/evidence
scripts/check.sh e2e
```

For one journey, with a report and traces kept as evidence:

```bash
PLAYWRIGHT_JSON_OUTPUT_NAME=docs/agent-tasks/<ID>/evidence/<journey>.json \
  npm --prefix apps/ui run e2e -- <journey>.spec.ts --trace on \
  --output docs/agent-tasks/<ID>/evidence/pw-<journey> --reporter=list,json
```

Replace `<ID>` and `<journey>` with the task ID and the journey file name before you run it.

- One `--output` folder per journey: Playwright empties its output folder at the start of
  a run, and a shared folder erases the earlier journey's files.
- Use roles and visible text, as the specs do. Do not edit a spec to make it pass; a red
  journey is a finding.
- Do not call the IPC client directly to create the state you are checking. Drive the
  screen.
- Take one screenshot per state the acceptance criteria name (loading, empty, error, done,
  and dark mode or a narrow width when the change touches them). Save them in the evidence
  folder. A fix that changes what is on screen needs new pictures from the final head.
- Check what the page shows against the AC, not only that the run was green.

## Cleanup

The run must leave nothing behind.

```bash
lsof -nP -iTCP:<port> -sTCP:LISTEN
```

It must print nothing. If a dev server you started is still listening, stop that process
by its PID, after you confirm with `lsof` that the PID is the one you started. Then list the
evidence folder and confirm the JSON report, the traces and the screenshots are still
there. A leftover listener or a missing report means the run is not done.

## Report

State the outcome first: VERIFIED (UI against the mock transport), NOT VERIFIED or BLOCKED.
Then a table: journey, wireframe flow, result, evidence path. Add the head commit
(`git rev-parse --short HEAD`), the port you checked and what `lsof` printed afterwards.
A journey you did not drive is "not driven". The real app, audio and latency are "not
covered by this run".
