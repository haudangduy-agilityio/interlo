# System Findings & Evidence Ledger: Desktop AI Meeting Copilot & Live Translation Architecture (`FINDINGS.md`)

- **Target Identifier**: `MeetingCopilot Desktop` (Sanitized Generic Identifier)
- **Primary Binary Architecture**: Mach-O ARM64 (`0x1000005a8`) / Electron Framework 32+ / Node.js runtime
- **Target SHA-256**: `ebdf088099cf62d0dc4315793d0860f0ac9cbd276948a001063a6ac976fab4b5`
- **Primary Evidence ID**: `ev_9b1b8eb3fba474431183977add65334a4d841644f78c6f404e6c30a39e70e766`
- **Methodology**: Reverse Engineering Assistant (REA) & REAP Investigation Methodology
- **Classification Schema**: `[CONFIRMED FACT]` | `[INFERENCE / HYPOTHESIS]` | `[UNKNOWN / GAP]`

---

## 1. Executive Summary & Architectural Overview

The target application is a cross-platform desktop application designed for **real-time multilingual meeting transcription, speech-to-speech live translation, automated note generation, in-meeting AI assistance, and system-wide voice dictation**.

```
                              ┌──────────────────────────────────────────────┐
                              │           macOS / Windows Desktop            │
                              └──────────────────────┬───────────────────────┘
                                                     │
                  ┌──────────────────────────────────┴──────────────────────────────────┐
                  │                                                                     │
                  ▼                                                                     ▼
       ┌────────────────────────┐                                            ┌─────────────────────┐
       │   Native Audio Loop    │                                            │ Global Accessibility│
       │  BlackHole 2ch/16ch    │                                            │  fn-key-listener    │
       │   (macOS) / VB-CABLE   │                                            │ (CGEventTap / AXUI) │
       │       (Windows)        │                                            └──────────┬──────────┘
       └──────────┬─────────────┘                                                       │
                  │                                                                     │
                  ▼                                                                     ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                     ELECTRON MAIN PROCESS                                       │
│  - Window Management: Frameless, transparent, floating pill/overlay, coordinate persistence    │
│  - Preload ContextBridge: IPC endpoints, update coordinator, permissions, audio driver check   │
│  - Local Auth Callback Server: Temporary HTTP listener for OAuth redirect flows                 │
│  - Learning Guard: Intercepts social distraction sites (TikTok/X/YouTube) via osascript        │
└─────────────────────────────────┬─────────────────────────────────────────────┬─────────────────┘
                                  │                                             │
                                  ▼ IPC Events                                  ▼ WebSockets / HTTP
┌────────────────────────────────────────────────────────┐ ┌──────────────────────────────────────┐
│                    RENDERER UI                         │ │            BACKEND SERVICE           │
│  - Framework: Next.js + React 18/19 + Redux + Tailwind │ │  - Express + Socket.IO (Port 5001)   │
│  - Audio Engine: WebAudio downsampler (16kHz PCM mono) │ │  - Dual Pipeline Architecture:       │
│  - Realtime Wavesurfer: Audio waveform synchronization │ │    1. Fast Realtime (OpenAI S2S)     │
│  - Notion-like Editor: Markdown + Lexical / Tiptap     │ │    2. Classic (Deepgram + Azure STT) │
│  - Floating Controller: HUD pinned over Zoom/Meet/Teams│ │  - Summarisation & BulletPoint Engine│
│  - Realtime S2S Player: Int16 PCM live audio playback  │ │  - MongoDB + Prisma Database Layer   │
└────────────────────────────────────────────────────────┘ └──────────────────────────────────────┘
```

---

## 2. REAP Evidentiary Ledger: 3-Bucket Classification

### Bucket 1: `[CONFIRMED FACT]`

1. **`[CONFIRMED FACT]` Application Packaging & Runtime**:
   - `ev_9b1b8eb3fba474431183977add65334a4d841644f78c6f404e6c30a39e70e766`: The main binary is a 64-bit Mach-O ARM64 executable linking `@rpath/Electron Framework.framework/Electron Framework`.
   - Source code analysis of `app.asar` reveals an unminified, full TypeScript/JavaScript project bundle consisting of `main/` (Electron main process), `renderer/` (Next.js frontend), and `server/` (local/backend microservice).

2. **`[CONFIRMED FACT]` Virtual Audio Capture & Driver Bundling**:
   - *Source File*: `main/background.ts#L75-L121`
   - On macOS, the app bundles and automatically detects `BlackHole 2ch` and `BlackHole 16ch` audio HAL plug-ins (`/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver`).
   - On Windows, the app bundles `VB-CABLE` (`VBCABLE_Setup_x64.exe`) and `Voice Audio Cable (VAC)`.
   - System audio loopback is achieved by routing conference audio (Zoom/Meet/Teams) through the virtual cable while capturing user microphone simultaneously through standard hardware audio input.

3. **`[CONFIRMED FACT]` System-Wide Dictation via Swift Native Binary**:
   - *Source File*: `main/native/fn-key-listener.swift#L1-L120`, `main/background.ts#L2187-L2189`
   - A compiled native Swift helper interacts with macOS Accessibility (`AXUIElement`) and `CGEventTapCreate`.
   - When the user presses the `Fn` key anywhere in the OS:
     - It queries `AXUIElementCopyAttributeValue(systemWide, kAXFocusedUIElementAttribute)` to identify the active input role (`kAXTextFieldRole`, `kAXTextAreaRole`, `kAXComboBoxRole`).
     - It records audio while `Fn` is held down, transmits PCM to the speech-to-text pipeline, formats the text via LLM, and inserts the result directly into the target app via clipboard paste simulation or direct accessibility attribute write.

4. **`[CONFIRMED FACT]` Real-Time Speech-to-Speech Translation Pipeline**:
   - *Source File*: `server/src/modules/realtimeTranslation/openAiRealtimeShared.ts#L24-L74`, `fastRealtimePipeline.ts#L32-L89`
   - Uses OpenAI Realtime WebSocket protocol (`wss://api.openai.com/v1/realtime/translations?model=gpt-realtime-translate`).
   - In transcription-only mode, uses `wss://api.openai.com/v1/realtime?intent=transcription` with `gpt-realtime-whisper`.
   - Audio input format: 24kHz 16-bit Mono PCM. Noise reduction mode: `near_field` for microphone stream, `far_field` for system meeting stream.

5. **`[CONFIRMED FACT]` High-Speed Linear PCM Resampler**:
   - *Source File*: `server/src/modules/realtimeTranslation/pcmResampler.ts#L1-L29`
   - Audio captured from browser/desktop at 16kHz is upsampled to 24kHz in pure Node.js Buffer operations using a custom linear interpolation algorithm:
     $$y = A + (B - A) \cdot \text{fraction}$$
     with 16-bit integer boundary clamping (`[-32768, 32767]`).

6. **`[CONFIRMED FACT]` Same-Language Output Suppression & Late Cancellation**:
   - *Source File*: `server/src/modules/realtimeTranslation/sameLanguageOutputGate.ts#L8-L100`
   - Implements `SameLanguageOutputGate` with `SAME_LANGUAGE_HOLD_MS = 500ms`, `SOURCE_WINDOW_MS = 2000ms`, `TRANSLATION_FRAME_MS = 200ms`.
   - Incoming translated speech audio frames are enqueued and held for 500ms while source text is evaluated. If source and target languages match (e.g. both Japanese), translated speech is discarded before playback. If already partially released, `onLateSuppress` revokes the audio playback.

7. **`[CONFIRMED FACT]` Dual-Stream TTS Loopback Suppression**:
   - *Source File*: `renderer/pages/_components/AudioRecorder.tsx#L65-L68`, `dualStreamTtsCaptureSuppression.ts`
   - When the app plays translated speech to the user, it suppresses the captured PCM chunks on the recording stream during playback time to avoid feedback loop acoustic bleed.

8. **`[CONFIRMED FACT]` Live In-Meeting Bullet Points Generation**:
   - *Source File*: `server/src/services/BulletPointService.ts#L38-L100`, `L250-L314`
   - Employs a dual-trigger architecture:
     1. Periodic 3-second evaluation timer (`PROCESSING_INTERVAL = 3000`).
     2. Voice Activity Detection (VAD) speech completion trigger (`onUtteranceEnd` event from Deepgram).
   - Prompts `gpt-4o-mini` with JSON output schema (`{ generate: boolean, summary?: string }`). Rejects filler words ("yeah", "okay") and only emits bullet points for meaningful insights, conclusions, or decisions.

9. **`[CONFIRMED FACT]` Notion-Style Rich Text Editor & Interactive Sync**:
   - *Source File*: `renderer/pages/_components/NotionEditor.tsx`, `renderer/components/desktop-home/DesktopRecordingDetail.tsx`
   - Integrates Tiptap and Lexical rich text editors supporting markdown shortcuts, heading blocks, task lists, and bilingual audio sync with `wavesurfer.js`.

10. **`[CONFIRMED FACT]` Micro-Learning Distraction Interceptor**:
    - *Source File*: `shared/learning.ts#L4-L66`, `main/helpers/learning-guard.ts#L20-L100`
    - Intercepts visits to entertainment domains (`tiktok.com`, `instagram.com`, `youtube.com`, `x.com`, `reddit.com`, `facebook.com`) via macOS AppleScript.
    - Suspends the distracting page and pops up an overlay language quiz derived from the user's meeting vocabulary or standard grammar banks before permitting access.

---

### Bucket 2: `[INFERENCE / HYPOTHESIS]`

1. **`[INFERENCE]` Production Server vs Bundled Server Hybrid Architecture**:
   - The repository contains a complete standalone Express/Socket.IO server in `server/`.
   - The desktop app can connect either to a locally spawned backend instance or to a cloud endpoint (`WEB_APP_BASE_URL`) based on environment configuration.
2. **`[INFERENCE]` Model Allocation by Tier**:
   - Free/Basic tiers utilize Deepgram `nova-3` for transcription and OpenAI `gpt-4o-mini` for translations/summaries.
   - Pro/Enterprise tiers unlock the ultra-low-latency `gpt-realtime-translate` OpenAI S2S WebSocket pipeline and ElevenLabs TTS.

---

### Bucket 3: `[UNKNOWN / GAP]`

1. **`[UNKNOWN / GAP]` Native Driver Auto-Silent Installation on Windows**:
   - The exact silent UAC bypass script for Windows driver installation was not fully traced, though helper scripts `windows-default-audio.ps1` handle device switching post-install.
2. **`[UNKNOWN / GAP]` Server-side Distributed Scalability**:
   - The extracted package includes single-node Socket.IO implementations; horizontal multi-node synchronization via Redis pub/sub exists in skeleton configuration but was disabled in local runs.

---

## 3. Deep Analysis of Prompt Engineering Systems

### 3.1 Meeting Note Summarization Prompt
Located in `server/src/services/SummarisationService.ts`:
```text
I will provide a user input including a notepad written during a meeting and a transcript of the entire meeting. The user input is in the following JSON format:
"{\"transcript\": \"[this transcript will be in markdown format]\", \"notepad\": \"[this notepad will be in markdown format]\"}"
Create a [gist] and [meetingNotes] in standard {TargetLanguage} adhering to the following definitions for each.
- [gist] definition: a short summary of the meeting as one plain text paragraph in standard {TargetLanguage}
- [meetingNotes] definition: an breakdown summary of the entire meeting [transcript] organised under markdown formatted sections and headings. The [notepad] should be taken as a guide to digest the [transcript] into each of the following sections:
  - [actionItems] definition: actions that have arisen from the meeting. Organise the action items under relevant headings (Heading 2 ##) informed by the [notepad]. Use bullet lists under these headings.
  - [keyPoints] definition: digest of key points from the meeting. Organise the key points under relevant headings (Heading 2 ##) informed by the [notepad]. Use bullet lists under these headings.
  - [otherPoints] definition: other information from the meeting that does not fit into the [keyPoints] and [actionItems] sections. Organise the other points under relevant headings (Heading 2 ##) informed by the [notepad]. Use bullet lists under these headings.
Please highlight short phrases in bold which closely match the [notepad] content by enclosing them in double asterisks (**). Use Headings 2 (##) and bullet lists in the markdown under each section to further organise the given sections.
Important: DO NOT repeat any version of the headings "Action Items", "Key Points", or "Other Points" in the target language, these headings are already hardcoded and this would result in repetition.
Make sure the whole response is in {TargetLanguage}.
```

### 3.2 Real-time In-Meeting Assistant (Ask AI) Prompt
Located in `server/src/modules/chat/chat.service.ts`:
```text
You are a REAL-TIME MEETING ASSISTANT. Users are actively in meetings and need IMMEDIATE, QUICK answers.

CRITICAL CONTEXT: You're helping someone DURING A LIVE MEETING. They can't read long responses.

RESPONSE RULES:
- Use bullet points for lists
- Bold only the MOST important keywords
- Be direct - no introductions or conclusions
- Only include external links if they are highly relevant and from well-known, verified sources
- Do not add links unless they directly support your answer or provide essential additional context
- LANGUAGE RULE: Always respond in the EXACT SAME LANGUAGE as the user's question.
- PRIORITY: Speed and clarity over completeness. Think of this as a meeting whisper, not a presentation.
```

### 3.3 System-Wide Voice Dictation Formatting Prompt
Located in `server/src/modules/user/dictationFormattingPrompt.ts`:
```text
You format speech-to-text for insertion into the user's currently focused app.
Return only the final text to insert, with no quotation marks, explanation, or Markdown fence.
Correct punctuation and obvious transcription errors while preserving meaning and language.
Use the focused application and input-role context to infer appropriate tone, capitalization, and layout.
When the speech clearly contains a list or sequence, use concise paragraphs and plain-text bullets or numbering; otherwise keep normal prose.
Treat spoken cues such as "new paragraph", "bullet point", and "numbered list" as formatting instructions instead of literal text.
Never add names, products, facts, or other content that is not present in the spoken dictation.
For password or secure contexts, return the transcript unchanged.
```

---

## 4. Audio Pipeline & Latency Optimization Algorithms

1. **Downsampler (`StreamingPcmDownsampler`)**:
   - Converts standard browser WebAudio capture (typically 44.1kHz or 48kHz float32) down to 16kHz 16-bit linear PCM in real-time chunks (typically 100ms – 250ms chunks).
2. **Resampler (`pcmResampler`)**:
   - Converts 16kHz 16-bit linear PCM to 24kHz 16-bit linear PCM to meet OpenAI Realtime API native input specifications.
3. **Echo & Bleed Suppression**:
   - Coordinates dual audio channels (`mic` and `system`).
   - Suppresses mic input when TTS output is active on the local speaker.
4. **Latency Measurement HUD**:
   - Captures timestamp deltas between `audio_chunk` emitted from client, STT transcription received, LLM translation delta received, and TTS audio chunk playback.
   - Typical latency metrics:
     - Classic Pipeline (Deepgram + GPT-4o-mini + TTS): 1200ms – 2200ms.
     - Fast Pipeline (OpenAI Realtime S2S WebSocket): 450ms – 850ms.
