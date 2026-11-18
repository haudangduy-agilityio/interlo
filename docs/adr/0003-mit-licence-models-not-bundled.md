# 0003. MIT licence, models not bundled

- Status: accepted
- Date: 2026-11-17
- Decision ID: D-39

## Context

Interlo is open source. The models it can run have different licences. Some
carry non-commercial or share-alike terms on their training data. Large model
files also do not belong in a source repository.

## Decision

License the code under the MIT licence. Do not bundle models in the repository
or in the app. The owner downloads each model from the Setup screen with a
click. A model that fails its checksum never runs.

## Consequences

- The code stays free to use, copy and change with few conditions.
- Each model keeps its own licence. Users read and accept those terms when they
  download a model. `docs/tech-stack.md` lists the model licences.
- The first run needs a download, and the app must work and explain itself
  before any model is present.
- Models, audio and databases are in `.gitignore` and must never be committed.
