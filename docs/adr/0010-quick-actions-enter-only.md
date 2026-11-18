# 0010. Quick actions paste only after Enter

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-45

## Context

An earlier idea let a second key replace the selected text at once, without a
preview. That path pastes without the owner's confirm, which guardrail 9
forbids.

## Decision

A quick action shows a preview. The text is pasted only after Enter. Esc
cancels. The "second key replaces at once" idea is removed everywhere, and there
is no setting to replace without a preview.

## Consequences

- Guardrail 9 holds with no exception.
- One extra keypress for the owner on every quick action.
- Hotkey docs list Enter and Esc and no second key.
- A test must show that nothing is pasted before Enter.
