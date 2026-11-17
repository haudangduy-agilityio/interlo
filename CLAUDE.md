# LiveMeetingCopilot - AI Assistant & Engineering Guidelines

## 1. Project Overview
`LiveMeetingCopilot` is a high-performance, real-time AI live translation, meeting transcription, and automated notes copilot built with React 19, TypeScript, Electron, Node.js, and WebSocket Speech-to-Speech (S2S) technologies.

---

## 2. Core Architecture & Research Index
All verified engineering discoveries, mathematical models, algorithms, and specifications are located in the `research/` directory. **ALWAYS refer to these documents before designing, refactoring, or implementing any features:**

- **[`research/SPEC.md`](./research/SPEC.md)**: Master Engineering Specification & Implementation Blueprint (Component hierarchy, dual pipeline, state machines).
- **[`research/FINDINGS.md`](./research/FINDINGS.md)**: Sổ cái sự thật kỹ thuật đã kiểm chứng (Verified Facts, runtime parameters, Evidence ID ledger).
- **[`research/MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md`](./research/MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md)**: Bóc tách 12 cụm tính năng kèm Sequence Diagrams (S2S Fast Mode, Classic Fallback, 3s Real-time Bullets, Ask Copilot).
- **[`research/ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md`](./research/ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md)**: 8 sơ đồ kiến trúc hệ thống (FSM máy trạng thái Socket.IO, Jitter Buffer, Prisma ERD, IPC contracts).
- **[`research/DEEP_ARCHITECTURE_ANALYSIS.md`](./research/DEEP_ARCHITECTURE_ANALYSIS.md)**: Mô hình toán học DSP (Continuous Box Filter, Session Pooling $O(L)$, Billing Multiplier).
- **[`research/OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md`](./research/OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md)**: Tích hợp sâu hệ điều hành (BlackHole/VB-CABLE Virtual Audio Drivers, Loopback capture ngầm, Global Fn key tap, Prompt Leak Guard).
- **[`research/CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md`](./research/CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md)**: Ma trận so sánh hệ sinh thái Desktop App vs Chrome Extension vs Mobile App.

---

## 3. Directory Layout & Architecture Rules
- **`src/`**: Pure production source code (`src/components/live-meeting-copilot/`, `src/lib/audio/`, `src/lib/realtime/`, `src/lib/classic/`).
- **`master-vault/`**: Curated, framework-ready components with standardized manifests (`master-vault/live-meeting-copilot/`).
- **`research/`**: Architectural documentation, specs, diagrams, and verified technical findings.
- **`data/`**: Standardized source inventory and raw extracted reference code.
- **ZERO-CLUTTER RULE**: Never create ad-hoc directories or leave scratch markdown files at the root.

---

## 4. Key Engineering Principles
1. **No Competitor References**: Always use generic functional terminology (`LiveMeetingCopilot`, `FloatingOverlay`, `DualPipeline`, `SameLanguageGate`). Never reference competitor trademarks.
2. **Audio DSP Standards**:
   - Audio input downsampling: 48kHz/44.1kHz Float32 $\to$ 16kHz/24kHz Int16 Mono (`StreamingPcmDownsampler`).
   - Jitter buffer: 4ms underrun grace time, 150ms tail playback (`PcmPlaybackQueue`).
   - Same-Language Gate: Suppresses translated speech when source language matches target language.
   - Prompt Leak Guard: `isLikelyVocabularyPromptLeak` prevents STT echo of vocabulary lists.

---

## 5. Development & Testing Commands
- **Run Unit Tests (DSP & Audio Engine)**:
  ```bash
  bun test src/lib/audio/__tests__/
  ```
- **Run Unit Tests (Realtime S2S Client)**:
  ```bash
  bun test src/lib/realtime/__tests__/
  ```
