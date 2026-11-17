# Agent Guidelines & Research Knowledge Map (`AGENTS.md`)

This file guides AI agents (Claude, Antigravity, Codex, Cursor, Windsurf) working on this codebase.

## 1. Ground Truth & Research Directory
All verified system designs, math models, and reverse-engineered specifications live in `research/`.
Before writing code or answering questions about features, read the relevant file:

1. **[`research/SPEC.md`](./research/SPEC.md)**: Main architecture blueprint, component hierarchy, dual-pipeline spec.
2. **[`research/FINDINGS.md`](./research/FINDINGS.md)**: Verified technical facts, parameters, and REAP evidence ledger.
3. **[`research/MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md`](./research/MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md)**: 12 core features breakdown & sequence diagrams.
4. **[`research/ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md`](./research/ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md)**: 8 architecture diagrams, FSM, Jitter buffer, Prisma schema, IPC contracts.
5. **[`research/DEEP_ARCHITECTURE_ANALYSIS.md`](./research/DEEP_ARCHITECTURE_ANALYSIS.md)**: Math models, DSP downsampling, session pool $O(L)$, billing multiplier.
6. **[`research/OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md`](./research/OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md)**: OS virtual audio drivers (BlackHole, VB-CABLE), loopback audio capture, native Fn key dictation.
7. **[`research/CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md`](./research/CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md)**: Desktop App vs Chrome Extension vs Mobile App matrix.

## 2. Directory Conventions
- `src/`: Clean-room implementation source code.
- `master-vault/`: Curated component vault.
- `research/`: Technical documentation and specs.
- `data/`: Standardized source packages.
- Root directory must remain clean and uncluttered.
