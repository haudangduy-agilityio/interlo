# Live Meeting Copilot & Speech-to-Speech Assistant (Deconstruction Package)

- **Source Identifier**: `live-meeting-copilot`
- **Architecture**: Electron Desktop (Mach-O ARM64) + Next.js / React 19 Frontend + Express / Socket.IO Backend
- **Target Category**: Real-time meeting transcription, speech-to-speech live translation, automated note generation, in-meeting AI assistance, and system-wide voice dictation.

---

## 1. Directory Structure

```
data/live-meeting-copilot/
├── components/           # Curated & sanitized React/TypeScript UI components
│   ├── AudioRecorder.tsx                # Dual-channel audio capture & streaming engine
│   ├── Controllers.tsx                  # Floating overlay pill/HUD controls
│   ├── Transcript.tsx                   # Live bilingual synchronized transcription stream
│   ├── Bullet.tsx                       # Real-time live AI summary bullet points
│   ├── Ask.tsx                          # In-meeting AI copilot (Ask Assistant)
│   ├── NotionEditor.tsx                 # Notion-like markdown rich-text notes editor
│   ├── LanguageSettingsPill.tsx         # Language selection pill
│   ├── TranslationLatencyMonitor.tsx    # Live latency HUD (ms metrics)
│   ├── RealtimeTranslatedSpeechPlayer.tsx # Int16 PCM translated speech playback
│   ├── DesktopRecordingDetail.tsx       # Post-meeting review & waveform inspection
│   ├── DesktopVoiceTyping.tsx           # Voice typing settings & Fn trigger
│   ├── DesktopChat.tsx                  # Meeting chat history & copilot
│   ├── DesktopHome.tsx                  # Main desktop dashboard view
│   ├── DesktopSidebar.tsx               # Collapsible desktop navigation bar
│   ├── DesktopShell.tsx                 # Main window frame & state coordinator
│   └── fn-key-listener.swift            # Native macOS Swift Fn key & Accessibility listener
├── pages/                # Desktop view templates
│   ├── home.tsx                         # Landing/Onboarding entry page
│   ├── dashboard.tsx                    # Main dashboard container
│   └── sign-up.tsx                      # Authentication view
├── research/             # In-depth architectural & reverse engineering reports
│   ├── FINDINGS.md                      # Verified Findings & 3-Bucket Evidence Ledger
│   ├── SPEC.md                          # Engineering Specification & Architecture Blueprint
│   ├── DEEP_ARCHITECTURE_ANALYSIS.md     # Deep-dive architecture, math models, audio routing & data graph
│   ├── MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md # Master feature-by-feature manual with Sequence Diagrams
│   ├── ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md # Comprehensive Mermaid architecture diagrams, state machines, jitter buffer & IPC/Socket contracts
│   ├── OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md # OS-level integration, virtual audio drivers, custom vocabulary injection & native media engineering
│   └── CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md # Multi-surface comparison: Desktop App vs Chrome Extension vs Mobile App
└── raw/                  # Extracted source code, manifests, and configs
    ├── package.json                     # Desktop app package manifest
    ├── asar_filelist.txt                # Index of all 10,996 files in target asar
    └── extracted/                       # Non-dependency source code tree (1,680 files)
        ├── main/                        # Electron main process (TS & Swift)
        ├── renderer/                    # Next.js / React renderer source
        ├── server/                      # Express + Socket.IO backend service
        └── shared/                      # Shared types, learning engines & vocabularies
```

---

## 2. Core Technical Innovations Uncovered

1. **Dual Real-Time Pipeline**:
   - Fast Pipeline: Direct WebSocket connection to OpenAI Realtime Speech-to-Speech API (`gpt-realtime-translate`), achieving sub-second voice-to-voice translation.
   - Classic Pipeline: Deepgram Nova-3 multilingual speech-to-text combined with streaming LLM translation and TTS fallback.
2. **Audio Routing & Echo Cancellation**:
   - Bundles BlackHole (macOS) and VB-CABLE (Windows) for conference audio loopback.
   - Implements `SameLanguageOutputGate` (500ms sliding hold) to suppress unnecessary translation when speakers share the same language.
   - Automatically suppresses mic input during local TTS playback to prevent feedback bleed.
3. **Live 3-Second In-Meeting Bullet Points**:
   - Triggers prompt evaluation every 3 seconds or immediately upon speaker pause (`onUtteranceEnd`), generating concise live meeting highlights without user intervention.
4. **System-Wide Dictation Hook**:
   - Native Swift binary utilizing `CGEventTap` and `AXUIElement` to stream speech-to-text directly into any focused application upon holding the `Fn` key.
5. **Micro-Learning Distraction Interceptor**:
   - Monitors frontmost browser tabs via AppleScript to pause social media browsing (TikTok/X/YouTube) and challenge the user with context-derived language quizzes.

---

## 3. License & Trademark Sanitization Notice

All third-party trademarks and competitor branding have been sanitized into neutral generic entities (`LiveMeetingCopilot`, `MeetingAssistant`). All third-party license keys and credentials have been scrubbed in compliance with repository governance rules.
