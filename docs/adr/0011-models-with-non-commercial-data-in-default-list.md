# 0011. Models trained on non-commercial data stay in the default list

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-47

## Context

Some candidate models, such as the translation models, were trained on data
under non-commercial terms (see `NOTICE`). Dropping them would leave v1 without
a working translation model. Models are not bundled (ADR 0003), so the owner
downloads each one.

## Decision

These models stay in the default model list. The model manager shows each
model's licence and its non-commercial flag, and the owner accepts the licence
before the download starts. README and NOTICE say so.

## Consequences

- The owner sees the terms before any download, and no model is bundled.
- The model table needs a licence text and a non-commercial flag per model.
- The licence screen is planned, not built.
