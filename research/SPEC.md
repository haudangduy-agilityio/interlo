# Engineering Specification: Modern Live Meeting Assistant (`SPEC.md`)

This document provides the complete architecture and implementation blueprint for rebuilding an equivalent or superior, high-performance, clean-room AI live meeting assistant using modern standards (React 19, TypeScript, Electron / Tauri, and WebSocket Realtime Speech-to-Speech).

---

## 1. System Architecture & Component Hierarchy

```
┌────────────────────────────────────────────────────────────────────────┐
│                        DESKTOP RUNTIME LAYER                           │
│  (Electron or Tauri with Rust Native Audio & Accessibility Tap)        │
│                                                                        │
│  ┌──────────────────────────┐           ┌────────────────────────────┐ │
│  │ Native Audio Loopback    │           │ Global OS Event Tap        │ │
│  │ CoreAudio HAL / WASAPI   │           │ CGEventTap / Windows Hook  │ │
│  └────────────┬─────────────┘           └──────────────┬─────────────┘ │
└───────────────┼────────────────────────────────────────┼───────────────┘
                ▼                                        ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        RENDERER UI LAYER                               │
│  - Modern React 19 + Tailwind CSS + Lucide Icons                       │
│  - Floating HUD Overlay: Frameless, transparent, draggable             │
│  - Bilingual Live Transcript Stream: Speaker diarization, real-time    │
│  - Notion-style Notes Editor: Rich Markdown, synced timestamps         │
│  - Realtime Wavesurfer: Audio playback & waveform inspection           │
│  - In-Meeting Copilot (Ask AI): Live contextual Q&A                    │
└─────────────────────────────────┬──────────────────────────────────────┘
                                  │ Binary WebSockets (PCM 16kHz)
                                  ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        REALTIME PIPELINE LAYER                         │
│                                                                        │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │ Audio Ingestion & Resampling Engine                            │   │
│   │ - WebAudio Downsampler: 48kHz Float32 -> 16kHz Int16 Mono      │   │
│   │ - Linear Resampler: 16kHz Int16 -> 24kHz Int16 (OpenAI spec)   │   │
│   │ - Echo & TTS Bleed Gate: Suppress mic feedback during TTS      │   │
│   └───────────────────────────────┬────────────────────────────────┘   │
│                                   │                                    │
│                 ┌─────────────────┴─────────────────┐                  │
│                 ▼                                   ▼                  │
│   ┌───────────────────────────┐       ┌────────────────────────────┐   │
│   │ Fast Pipeline (S2S)       │       │ Classic Pipeline (Fallback)│   │
│   │ OpenAI Realtime WebSocket │       │ Deepgram Nova-3 (STT)      │   │
│   │ - gpt-realtime-translate  │       │ + GPT-4o-mini (Translate)  │   │
│   │ - gpt-realtime-whisper    │       │ + ElevenLabs / Deepgram TTS│   │
│   │ - Same-Language Filter    │       │ - UtteranceEnd VAD Trigger │   │
│   └───────────────────────────┘       └────────────────────────────┘   │
│                                   │                                    │
│                                   ▼                                    │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │ Live Summarization & Bullet Points Engine                      │   │
│   │ - 3-second evaluation loop for meaningful insights             │   │
│   │ - Post-meeting structured notes worker with Mongo/Prisma       │   │
│   └────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Core Functional Modules to Implement

### Module 1: Floating Overlay Window (`Controllers.tsx`)
- **Visual Presentation**:
  - Pill-shaped HUD floating at top-center or pinned beside Zoom/Teams/Meet.
  - Transparent backdrop (`backgroundColor: '#00000000'`), border radius `rounded-full` or `rounded-2xl`, glassmorphic blur (`backdrop-blur-md`).
- **Interactive Controls**:
  - Start / Pause / Stop recording toggle button.
  - Source Language / Target Language quick selector pills.
  - Speech-to-Speech audio toggle (Mute / Unmute translated voice).
  - Live transcription latency badge (showing real-time ms delay).
  - Quick access to Notepad / Meeting Copilot sidebar.

### Module 2: Audio Routing & Capture Engine
- **Input Channels**:
  - `mic`: Physical microphone via `navigator.mediaDevices.getUserMedia({ audio: { sampleRate: 16000, channelCount: 1, echoCancellation: true } })`.
  - `system`: Conference output captured via virtual audio cable (BlackHole on macOS, VB-CABLE on Windows).
- **Processing**:
  - Linear PCM Resampler (`resamplePcm16From16kTo24k`):
    $$y[n] = A + (B - A) \cdot \text{fraction}$$
  - Same-Language Suppression Gate:
    Enqueues outgoing translated audio for 500ms; if source transcript language matches the listener's target language, suppresses playback to prevent echoing.

### Module 3: Dual-Mode Speech-to-Speech & STT Pipeline
1. **Ultra-Low Latency Pipeline (`FastPipeline`)**:
   - Connection: Persistent WebSocket to `wss://api.openai.com/v1/realtime/translations?model=gpt-realtime-translate`.
   - Framing: Manual commits with 100ms–200ms audio buffers.
   - Output: Concurrent streaming of both translated text delta (`onOutputTranscriptDelta`) and 24kHz PCM audio delta (`onOutputAudioDelta`).
2. **High-Accuracy Fallback Pipeline (`ClassicPipeline`)**:
   - Deepgram `nova-3` for streaming STT.
   - OpenAI `gpt-4o-mini` with streaming SSE for sentence-by-sentence translation.
   - Deepgram Aura / ElevenLabs for TTS generation.

### Module 4: Live AI Summarization & Copilot
1. **Live Bullet Points Generator (`BulletPointService`)**:
   - Triggers: Periodic 3s buffer check OR VAD `onUtteranceEnd` event.
   - Prompt format: Structured JSON `{ generate: boolean, summary?: string }`.
   - Rejects filler words ("ok", "yeah") and ensures ultra-concise output in target language.
2. **Interactive In-Meeting Copilot (`AskAI`)**:
   - RAG over real-time meeting transcript window.
   - Rule: "Speed and clarity over completeness. Think of this as a meeting whisper, not a presentation."
3. **Post-Meeting Curated Notes (`SummarisationService`)**:
   - Synthesizes user's personal notepad + full transcript.
   - Emits structured JSON: `{ gist: string, actionItems: string, keyPoints: string, otherPoints?: string }`.
   - Highlights user notes in bold (`**...**`).

### Module 5: System-Wide Dictation via OS Event Tap
- **Native Implementation**:
  - Global `Fn` or `Ctrl+Space` keyboard hook via `CGEventTapCreate` (macOS) / `SetWindowsHookEx` (Windows).
  - Detect focused input field (`kAXFocusedUIElementAttribute`).
  - Capture audio while held down -> Stream to Whisper/Nova-3 -> LLM post-formatting prompt -> Simulate paste or set AX value directly.

---

## 3. Standard IPC & WebSocket Protocol Contracts

### 3.1 WebSocket Messages (Client <-> Server)
- **Client to Server**:
  - `audio_chunk`: Binary Buffer (Int16 PCM).
  - `start_recording`: `{ meetingId: string, spokenLanguage: string, targetLanguage: string, mode: 'fast' | 'classic' }`.
  - `stop_recording`: `{ meetingId: string }`.
  - `ask_ai`: `{ question: string, meetingId: string, transcriptSlice: Array<{ speaker: string, text: string }> }`.
- **Server to Client**:
  - `pipeline_mode`: `{ mode: 'realtime-fast' | 'classic', model: string }`.
  - `realtime_transcript_delta`: `{ delta: string, isFinal: boolean, speaker: string, sectionId: number }`.
  - `realtime_audio_delta`: `{ pcmBase64: string, sequence: number, elapsedMs: number }`.
  - `bullet_point`: `{ text: string, timestamp: string, speaker: string }`.
  - `meeting_notes_ready`: `{ gist: string, actionItems: string, keyPoints: string }`.

---

## 4. UI/UX Component Specifications

1. **`Controllers.tsx`**: Floating pill bar, 40px height, responsive width 320px–420px.
2. **`Transcript.tsx`**: Virtualized list rendering bilingual speech balloons (original text on top, translated text below).
3. **`NotionEditor.tsx`**: Block-based rich text editor with real-time markdown parsing.
4. **`DesktopRecordingDetail.tsx`**: 3-tab layout:
   - Tab 1: `my-notes` (Personal notepad).
   - Tab 2: `enhanced` (AI structured summary).
   - Tab 3: `transcript` (Interactive synchronized audio waveform + transcript).
