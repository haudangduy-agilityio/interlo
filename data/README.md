# Master Source Inventory (`data/README.md`)

This directory contains standardized, sanitized, and reverse-engineered source packages for reference, clean-room reconstruction, and component integration.

## Registered Sources

| Source Name | Category | Primary Stack | Key Highlights | Status |
| :--- | :--- | :--- | :--- | :--- |
| [`live-meeting-copilot`](./live-meeting-copilot/README.md) | AI Meeting Assistant & Live Translation | Electron (ARM64), Next.js, React 19, Socket.IO, Swift | Realtime S2S (OpenAI Realtime WebSocket), BlackHole/VB-CABLE audio loopback, Live 3s bullet points, System-wide Fn dictation, Notion-like editor | **Curated & Ready** |

---

## Directory Standards for Sources

Every source registered under `data/<source_name>/` adheres strictly to the 5 standard directories:
1. `components/`: Clean-room sanitized reusable UI components (`.tsx`, `.jsx`, `.css`).
2. `pages/`: Full page layouts and view templates.
3. `research/`: Formal engineering reports (`FINDINGS.md`, `SPEC.md`).
4. `raw/`: Extracted codebases, manifests, and reference configurations.
5. `README.md`: Overview and integration guide.
