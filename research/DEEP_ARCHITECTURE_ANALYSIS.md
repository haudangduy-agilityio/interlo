# Deep Technical Architecture & Algorithm Analysis: AI Live Meeting Assistant

- **Target Identifier**: `LiveMeetingCopilot` (Sanitized Generic Reference)
- **Document Type**: Comprehensive Deep-Dive Architecture & Algorithm Dossier
- **Methodology**: Reverse Engineering Assistant (REA) / REAP Evidence-Backed Investigation
- **Target Source Path**: `data/live-meeting-copilot/raw/extracted/`

---

## 1. Multi-Guest Real-Time Translation & Language-Pooled Architecture

### 1.1 The N-to-Language Scaling Problem
In a live meeting with 1 host and $N$ remote web guests (each potentially speaking or reading a different language), spawning a dedicated AI speech-to-speech session per user creates massive resource exhaustion ($O(N)$ API cost and latency jitter).

The system solves this via **Language Pooling** implemented in `webGuestRealtimeSessionPool.ts`:

```
                                  ┌──────────────────────────┐
                                  │   Desktop Host Session   │
                                  │ (Streams 16kHz PCM audio)│
                                  └─────────────┬────────────┘
                                                │
                     ┌──────────────────────────┴──────────────────────────┐
                     │                                                     │
                     ▼                                                     ▼
           [Host Mic Stream]                                     [Host System Stream]
                     │                                                     │
                     └──────────────────────────┬──────────────────────────┘
                                                │
                                                ▼
                       ┌──────────────────────────────────────────────────┐
                       │           Web Guest Session Pool Manager         │
                       │          (webGuestRealtimeSessionPool.ts)        │
                       └────────────────────────┬─────────────────────────┘
                                                │
                 ┌──────────────────────────────┼──────────────────────────────┐
                 │                              │                              │
                 ▼                              ▼                              ▼
      ┌────────────────────┐         ┌────────────────────┐         ┌────────────────────┐
      │  Japanese Pool (1) │         │ Vietnamese Pool (1)│         │  Spanish Pool (1)  │
      │ gpt-realtime-trans │         │ gpt-realtime-trans │         │ gpt-realtime-trans │
      └──────────┬─────────┘         └──────────┬─────────┘         └──────────┬─────────┘
                 │                              │                              │
        ┌────────┴────────┐            ┌────────┴────────┐            ┌────────┴────────┐
        │        │        │            │        │        │            │        │        │
        ▼        ▼        ▼            ▼        ▼        ▼            ▼        ▼        ▼
     Guest 1  Guest 2  Guest 3      Guest 4  Guest 5  Guest 6      Guest 7  Guest 8  Guest 9
     (Web)    (Web)    (Web)        (Web)    (Web)    (Web)        (Web)    (Web)    (Web)
```

### 1.2 Mathematical Complexity & Cost Savings
Let:
- $N$ be total web guests joining a meeting.
- $L$ be the count of distinct target languages selected by guests ($L \le N$, typically $L \ll N$).
- $S$ be active audio streams (`mic`, `system`, $S \in \{1, 2\}$).

Standard 1:1 Architecture:
$$\text{Sessions}_{\text{naive}} = N \cdot S$$

Language-Pooled Architecture:
$$\text{Sessions}_{\text{pooled}} = L \cdot S$$

For a 50-person multinational meeting where participants view 3 target languages (Japanese, Vietnamese, English), session count drops from $50 \times 2 = 100$ down to $3 \times 2 = 6$, delivering an **over 94% reduction** in OpenAI Realtime API bandwidth and token costs.

### 1.3 Guest Connection Lifecycle & Failover Recovery
- **Concurrency Gate**: `GUEST_CONNECT_CONCURRENCY = 4` prevents burst connection stampedes when dozens of guests click a shared link simultaneously.
- **Audio First Chunk Deadline**: `GUEST_AUDIO_FIRST_CHUNK_DEADLINE_MS = 1500ms`. If audio flow is delayed, the session marks a diagnostic warning.
- **HTTP Fallback**: `GUEST_CONNECT_HTTP_FALLBACK_MS = 2500ms`. If the WebSocket handshake fails or stalls for 2.5 seconds, the guest is transparently downgraded to the classic HTTP streaming translation pipeline.
- **Reconnection Backoff**: Exponential backoff capped at `GUEST_RECONNECT_MAX_DELAY_MS = 10000ms`.

---

## 2. Audio Routing Auto-Configuration & Virtual Cable Injection

### 2.1 Virtual Device Detection Heuristics
Located in `renderer/utils/audioRoutingAutoConfig.ts`:
- **macOS Loopback**: Uses regular expressions to match BlackHole virtual drivers:
  ```typescript
  /blackhole/i.test(deviceLabel)
  ```
- **Windows Loopback**: Uses comprehensive patterns covering VB-Audio, VAC, and Stereo Mix:
  ```typescript
  /vb-audio|vb-cable|virtual audio cable|\bVAC\b/i.test(deviceLabel)
  /^(default\s*[-–—]\s*)?CABLE(-A)?\s+(Input|Output)\b/i
  /line\s*1.*virtual audio cable|virtual audio cable.*line\s*1/i
  /stereo mix|what u hear|loopback|wave out|mix/i
  ```

### 2.2 Bidirectional Speech-to-Speech Modes (`SpeechToSpeechMode`)
The engine supports 5 operational modes for real-time translation:

| Mode | Audio Flow Description | Primary Use Case |
| :--- | :--- | :--- |
| `off` | Live transcription and text translation only; no synthetic voice output. | Quiet meetings, webinars, silent note-taking. |
| `online-incoming` | Captures meeting audio (`system` via BlackHole), translates foreign speakers, and plays English audio into the user's headphones. | Listening to foreign-language presentations. |
| `online-outgoing` | Captures user's mic, translates user's voice, and injects synthetic speech into `CABLE Input` / `BlackHole` so meeting participants hear the translated voice. | Speaking into foreign-language meetings. |
| `online-both` | Full duplex: translates foreign participants into user's ear, AND translates user's speech into the meeting audio channel. | Fully interactive multilingual meetings. |
| `in-person` | Uses local physical mic(s) on a table between two people; plays translations out loud on speaker. | Face-to-face travel, interviews, medical visits. |

### 2.3 Acoustic Loopback & Echo Suppression
A critical vulnerability in desktop speech-to-speech tools is **acoustic feedback loop**:
When the app plays translated audio into the speakers or virtual microphone, the microphone recaptures the translated sound and sends it back to STT, creating infinite translation babble.

The system implements a three-tier suppression mechanism:
1. **Physical Device Filter**: Distinguishes hardware inputs (`isPhysicalCaptureDevice`) from virtual software inputs.
2. **Dual-Stream TTS Suppression (`dualStreamTtsCaptureSuppression.ts`)**:
   - When outgoing TTS audio is actively rendering, the capture buffer enqueues silence or ignores the microphone PCM frame for the exact duration of the playback:
     $$\text{suppress}(t) = \mathbb{I}(t \in [t_{\text{start\_tts}}, t_{\text{end\_tts}} + \delta_{\text{decay}}])$$
3. **500ms Same-Language Output Gate (`SameLanguageOutputGate.ts`)**:
   - Delays release of translated speech by `SAME_LANGUAGE_HOLD_MS = 500ms`.
   - If language evidence from the incoming source transcript proves the speaker is already speaking the target language, the translation frame is dropped before reaching the audio driver.

---

## 3. Billing, Metering & Usage Multiplier Engine

### 3.1 Tiered Multiplier Matrix
Located in `server/src/helpers/translationUsageMultiplier.ts`:

```
                                  ┌─────────────────────────────┐
                                  │   Meeting Duration (Time)   │
                                  └──────────────┬──────────────┘
                                                 │
                        ┌────────────────────────┴────────────────────────┐
                        ▼                                                 ▼
             [Fast Realtime S2S]                               [Classic STT + LLM]
                        │                                                 │
                        ▼                                                 ▼
               Flat 5× Multiplier                                Base STT Provider
             (Math.ceil(min * 5))                                         │
                        │                                 ┌───────────────┴───────────────┐
                        │                                 ▼                               ▼
                        │                        Deepgram Nova-3 (1×)           Microsoft Azure (2×)
                        │                                 │                               │
                        │                                 └───────────────┬───────────────┘
                        │                                                 │
                        │                                                 ▼
                        │                                     Multi-Target Compounding
                        │                                    base × (1 + ExtraLanguages)
                        │                                                 │
                        ▼                                                 ▼
           ┌────────────────────────────────────────────────────────────────────────┐
           │                     Total Billable Minutes Deducted                    │
           └────────────────────────────────────────────────────────────────────────┘
```

- **Session Snapshot Locking (`setSessionBillingSnapshot`)**:
  When a Fast Realtime meeting begins with translation enabled, the 5× multiplier is locked into the session state. If the user toggles translation off late in the meeting, the session maintains the fast tier rate for proper allocation of reserved OpenAI capacity.
- **Fractional Minute Computation**:
  $$\text{Minutes}_{\text{Fast}} = \left\lceil \frac{\text{AudioDurationMs}}{60,000} \times 5 \right\rceil$$

---

## 4. Real-Time Transcript Merging Algorithm

### 4.1 Zero-Flicker Streaming Alignment
Located in `renderer/utils/fastRealtimeTranscriptMerge.ts`:
Streaming speech recognition generates frequent token corrections and revisions. If rendered naively, text jumps, disappears, or repeats words.

The alignment algorithm uses `findTargetIndex`:
```typescript
function findTargetIndex(prev: any[], payload: FastRealtimeMergePayload, stream: FastRealtimeStreamId): number {
  // 1. Direct correlation via sectionId
  if (typeof payload.sectionId === 'number') {
    const byId = prev.findIndex((section) => section?.sectionId === payload.sectionId);
    if (byId >= 0) return byId;
  }

  // 2. Stream-based backwards search for active unfinalized segment
  for (let i = prev.length - 1; i >= 0; i -= 1) {
    if (prev[i]?.desktopAudioSource !== stream) continue;
    if (prev[i]?.fastSectionFinalized) continue;
    return i;
  }

  return -1; // Create new speech bubble
}
```

### 4.2 Emission Throttling & Debounce Constants
- `PARTIAL_EMIT_THROTTLE_MS = 50ms`: Caches incoming WebSocket deltas and flushes them to React state at a 20Hz cap, matching the browser render cycle and eliminating CPU spikes.
- `INCOMING_PARTIAL_FINALIZE_DEBOUNCE_MS = 800ms`: When foreign speech from Zoom stops for 800ms, the section is automatically marked final and handed to the TTS playback queue.
- `PARTIAL_FINALIZE_DEBOUNCE_MS = 1500ms`: Used for microphone input to permit natural speaker pauses without prematurely cutting off sentences.

---

## 5. System-Wide Voice Dictation (Swift CGEventTap + AXUIElement)

### 5.1 Native Hook Architecture (`fn-key-listener.swift`)
The desktop client features system-wide voice typing anywhere in macOS without requiring focus on the app window.

```
[User presses Fn Key] ──> [CGEventTap (flagsChanged)] ──> [Emit to Electron Main via stdout]
                                                                     │
                                                                     ▼
[Find Focused Field]  <── [AXUIElementCopyAttributeValue] <── [Electron spawns Dictation Audio]
(kAXTextFieldRole)                                                   │
                                                                     ▼
                                                          [Record PCM & stream to Whisper]
                                                                     │
                                                                     ▼
                                                          [LLM Post-Formatting Prompt]
                                                                     │
                                                                     ▼
[AXUIElementSetAttributeValue / Paste Simulation] <──────── [Insert clean formatted text]
```

### 5.2 Accessibility Fallback & Non-Observable Targets
Not all macOS apps expose standard accessibility attributes (e.g. Electron apps with disabled AX, terminal emulators, games).
The Swift listener handles this via `PasteAccessibilityObservation`:
1. Attempts direct attribute assignment via `AXUIElementSetAttributeValue(element, kAXValueAttribute, newText)`.
2. If the application rejects AX manipulation or is marked unobservable (`isTrustedUnobservableDictationTarget`), it falls back to saving the system clipboard, copying the formatted text to clipboard, triggering synthetic `Cmd+V` keypresses via `CGEventCreateKeyboardEvent`, and restoring the original clipboard content.

---

## 6. Complete Data Domain Architecture (Prisma Schema Highlights)

The backend data architecture (`server/prisma/schema.prisma`) comprises over 75 interconnected models structured into 6 primary operational clusters:

1. **Meeting & Collaboration Cluster**:
   - `Meeting`: Stores title, audio URLs, transcript JSON, notepad, attendee lists, and AI summary status.
   - `LiveTranslationInvitation`: Public shared meeting codes, allowed languages, and expiry dates.
   - `BulletPoints`: Time-indexed live highlights linked to `Meeting`.
   - `MeetingComment`: Inline comments on specific transcript timestamps.
2. **User & Identity Cluster**:
   - `User`: Profile, preferred spoken/notes languages, subscription tier, Google OAuth credentials.
   - `GoogleCalendarConnection`: OAuth tokens and sync anchors for automatic meeting detection.
   - `UserVocabulary`: Custom phrase dictionaries and company jargon with pronunciation guides.
3. **Billing & Subscriptions Cluster**:
   - `Subscription`, `MinutesSubscription`, `MonthlyUsage`, `MonthlyMinutesUsage`: Multi-bucket billing tracking raw transcription minutes, translation minutes, and AI credits with Stripe webhook integration.
4. **Context & Personalization Cluster**:
   - `OperationalContext`: Company mission, team roles, and domain background fed into LLM prompts to tailor meeting notes.
   - `CustomerProfileInsight`: Automatically generated CRM profiles of meeting attendees based on historical conversations.
5. **Media Dubbing & Offline Translation Cluster**:
   - `FileTranslation`, `ImageTranslation`, `VideoDubbingJob`, `AudioDubbingJob`: Async media translation pipelines storing source/result keys on AWS S3.
6. **Micro-Learning Gamification Cluster**:
   - `LearningPlayer`, `LearningQuestionBank`, `LearningCorrectAnswer`: Distraction blocker question database and user mastery tracking.
