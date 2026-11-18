<!-- Populate with evidence from actual code/CI; remove unused sections and all comments. Do not claim PASS without proof. -->
**Task:** <!-- ticket URL, or remove -->

**Status:** <!-- DRAFT / READY; BLOCKED must not be marked Ready -->

## Bug / Root Cause / Fix
**Symptom:** <!-- user-visible failure -->

**Root cause:** <!-- confirmed cause -->

**Fix:** <!-- new behavior -->

## Regression Proof
| Scenario | Before | After | Evidence |
|---|---|---|---|
| <!-- reproduction --> | <!-- expected failure --> | <!-- PASS --> | <!-- link --> |

## Implementation Flow
<!-- Embed a concise verified Mermaid diagram or linked SVG/PNG, only if useful. -->

## Review Carefully
<!-- 0–5 concrete human-focused points, file/commit link + decision/question. If material unresolved decisions exist, add Decision Required before this section and keep Draft. -->

## Automated Gates & Verification
| Gate / Check | Status | Evidence |
|---|---|---|
| Relevant tests / TDD | <!-- PASS / FAIL / NOT RUN / N/A --> | <!-- CI link --> |
| Runtime / behavior | <!-- status --> | <!-- evidence --> |
| Code review | <!-- status --> | <!-- report --> |
| Security review | <!-- status --> | <!-- report --> |
| Architecture review | <!-- status --> | <!-- report --> |
| Performance review | <!-- status; N/A when the review scope rule skips it --> | <!-- report; findings with verdict and outcome --> |

**Verified commit:** `<!-- SHA -->`

<!-- For Draft only: add Decision Required ABOVE Review Carefully. Include owner, options, impact, and approval needed. For Ready: omit if none. -->

## Irreversible After Merge
<!-- Keep ONLY when a revert will not restore the old state: migration, data change, public contract, anything already sent or published. Otherwise delete this section. -->
**Stays after a revert:** <!-- what cannot be undone, and who or what it affects -->

**Recovery:** <!-- how to repair it, or "none" -->

## Risks / Deployment Notes
<!-- Residual risk, config, rollout/rollback when relevant; remove if none. -->
