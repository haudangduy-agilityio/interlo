# LiveMeetingCopilot - AI Assistant & Engineering Guidelines

## 1. Project Overview
`LiveMeetingCopilot` is a high-performance, real-time AI live translation, meeting transcription, and automated notes copilot built with React 19, TypeScript, Electron, Node.js, and WebSocket Speech-to-Speech (S2S) technologies.

---

## 2. Directory Layout & Architecture Rules
- **`src/components/interlo/`**: UI components.
- **`src/lib/audio/`**: Audio DSP (downsampling, resampling, jitter buffer, same-language gate).
- **`src/lib/realtime/`**: Realtime S2S client and audio pipeline.
- **`src/lib/classic/`**: Classic (non-S2S) fallback pipeline.
- **ZERO-CLUTTER RULE**: Never create ad-hoc directories or leave scratch files at the repo root.

---

## 3. Key Engineering Principles
1. **No Competitor References**: Always use generic functional terminology (`LiveMeetingCopilot`, `FloatingOverlay`, `DualPipeline`, `SameLanguageGate`). Never reference competitor trademarks.
2. **Audio DSP Standards**:
   - Audio input downsampling: 48kHz/44.1kHz Float32 to 16kHz/24kHz Int16 Mono (`StreamingPcmDownsampler`).
   - Jitter buffer: 4ms underrun grace time, 150ms tail playback (`PcmPlaybackQueue`).
   - Same-Language Gate: Suppresses translated speech when source language matches target language.
   - Prompt Leak Guard: `isLikelyVocabularyPromptLeak` prevents STT echo of vocabulary lists.

---

## 4. Development & Testing Commands
```bash
bun test src/lib/audio/__tests__/
bun test src/lib/realtime/__tests__/
```
