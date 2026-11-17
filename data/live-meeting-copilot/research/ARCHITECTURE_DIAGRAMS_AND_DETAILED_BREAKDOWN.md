# Sơ Đồ Kiến Trúc Hệ Thống & Bóc Tách Chi Tiết Từng Tính Năng (Architecture Diagrams & Comprehensive Feature Breakdown)

- **Mã định danh dự án**: `LiveMeetingCopilot Core Engine` (Generic Sanitized Entity)
- **Phương pháp tiếp cận**: Reverse Engineering Assistant (REA) / REAP / Clean-Room Systems Architecture
- **Nguồn chứng cứ**: Bóc tách từ 1.680 file mã nguồn thực tế tại `data/live-meeting-copilot/raw/extracted/`.

---

## MỤC LỤC BẢN THIẾT KẾ & SƠ ĐỒ HỆ THỐNG
1. [Sơ Đồ 1: Cấu Trúc Đa Tầng Của Tiến Trình Desktop (Multi-Process Desktop Topology)](#1-sơ-đồ-1-cấu-trúc-đa-tầng-của-tiến-trình-desktop-multi-process-desktop-topology)
2. [Sơ Đồ 2: Chu Trình Xử Lý Âm Thanh Hai Luồng (Dual-Channel Audio Pipeline & Jitter Buffer)](#2-sơ-đồ-2-chu-trình-xử-lý-âm-thanh-hai-luồng-dual-channel-audio-pipeline--jitter-buffer)
3. [Sơ Đồ 3: Máy Trạng Thái Giao Thức OpenAI Realtime WebSocket (State Machine)](#3-sơ-đồ-3-máy-trạng-thái-giao-thức-openai-realtime-websocket-state-machine)
4. [Sơ Đồ 4: Kiến Trúc Phím Tắt Fn Toàn Hệ Thống (Native Swift EventTap & Accessibility)](#4-sơ-đồ-4-kiến-trúc-phím-tắt-fn-toàn-hệ-thống-native-swift-eventtap--accessibility)
5. [Sơ Đồ 5: Hồ Phiên Đa Ngôn Ngữ Tiết Kiệm 94% Chi Phí (Language-Pooled Web Relay)](#5-sơ-đồ-5-hồ-phiên-đa-ngôn-ngữ-tiết-kiệm-94-chi-phí-language-pooled-web-relay)
6. [Sơ Đồ 6: Vòng Đời Tóm Tắt Tức Thì 3 Giây (Live 3s Bullet Point Life Cycle)](#6-sơ-đồ-6-vòng-đời-tóm-tắt-tức-thì-3-giây-live-3s-bullet-point-life-cycle)
7. [Sơ Đồ 7: Pipeline Xử Lý File Kéo Thả 27 Định Dạng (Direct S3 Staging & Dubbing)](#7-sơ-đồ-7-pipeline-xử-lý-file-kéo-thả-27-định-dạng-direct-s3-staging--dubbing)
8. [Sơ Đồ 8: Sơ Đồ Thực Thể Cơ Sở Dữ Liệu Lõi (Prisma ERD Diagram)](#8-sơ-đồ-8-sơ-đồ-thực-thể-cơ-sở-dữ-liệu-lõi-prisma-erd-diagram)
9. [Bảng Đặc Tả Toàn Bộ Hợp Đồng Sự Kiện IPC & WebSocket (Event Contracts)](#9-bảng-đặc-tả-toàn-bộ-hợp-đồng-sự-kiện-ipc--websocket-event-contracts)

---

# 1. Sơ Đồ 1: Cấu Trúc Đa Tầng Của Tiến Trình Desktop (Multi-Process Desktop Topology)

Hệ thống desktop phân chia ranh giới nghiêm ngặt giữa **3 tiến trình con chạy độc lập**, giao tiếp bất đồng bộ:

```mermaid
graph TD
    subgraph KERNEL_SPACE["TẦNG HỆ ĐIỀU HÀNH & KERNEL (macOS / Windows)"]
        CORE_AUDIO["CoreAudio / WASAPI Audio Engine"]
        HAL_DRIVER["Virtual Audio HAL Driver (BlackHole / VB-CABLE)"]
        CG_SERVER["WindowServer & Accessibility Daemon (AXUIElement)"]
    end

    subgraph SWIFT_PROCESS["TIẾN TRÌNH NATIVE (Helper Child Process)"]
        SWIFT_DAEMON["fn-key-listener.swift Binary"]
        CG_TAP["CGEventTap (Lắng nghe .flagsChanged toàn cục)"]
        AX_PROBER["AXUIElement Prober (Tìm focused input element)"]
        PASTE_OBSERVER["PasteAccessibilityObservation (Giả lập Cmd+V / Set AXValue)"]
    end

    subgraph ELECTRON_MAIN["TIẾN TRÌNH ELECTRON MAIN (Node.js Background Process)"]
        APP_ENTRY["background.ts (Entry Point & Lifecycle)"]
        WIN_CONTROLLER["Window Bounds Controller (Persist Home vs Recording bounds)"]
        LOCAL_AUTH_SRV["Local Auth HTTP Server (Port 8000 for OAuth callback)"]
        LEARNING_GUARD["Learning Guard (osascript browser tab prober)"]
        PRELOAD_API["preload.ts (ContextBridge Sandbox API)"]
    end

    subgraph RENDERER["TIẾN TRÌNH RENDERER (Chromium Webview - Next.js)"]
        UI_STORE["Redux Toolkit Store (Recorder, Sharing, Languages)"]
        CONTROLLER_HUD["Controllers.tsx (Floating Pill Overlay HUD)"]
        TRANSCRIPT_LIST["Transcript.tsx (Bilingual Virtualized Dialogue Stream)"]
        NOTION_NOTE["NotionEditor.tsx (Tiptap / Lexical Rich Editor)"]
        AUDIO_WORKLET["AudioWorklet / ScriptProcessor (16kHz PCM Streamer)"]
        PCM_QUEUE["PcmPlaybackQueue.ts (Jitter Buffer & AudioContext Sink)"]
    end

    subgraph BACKEND_SRV["TIẾN TRÌNH SERVER NỘI BỘ (Express + Socket.IO Port 5001)"]
        SOCKET_IO["Socket.IO Server (/socket.io)"]
        FAST_PIPELINE["FastRealtimePipeline (OpenAI WebSocket Client)"]
        CLASSIC_PIPELINE["ClassicPipeline (Deepgram Nova-3 STT + TTS)"]
        GATE_KEEPER["SameLanguageOutputGate (Sliding 500ms Suppress Filter)"]
        NOTES_WORKER["PostMeetingNotes Worker (MongoDB queue lock)"]
    end

    CORE_AUDIO <-->|Hardware I/O| HAL_DRIVER
    CG_SERVER <-->|Event Tap| CG_TAP
    CG_SERVER <-->|Attribute Queries| AX_PROBER

    CG_TAP --> SWIFT_DAEMON
    AX_PROBER --> SWIFT_DAEMON
    SWIFT_DAEMON -->|stdout: 'fn-down\trole'| APP_ENTRY
    APP_ENTRY -->|stdin: 'insert-text\tcontent'| SWIFT_DAEMON
    SWIFT_DAEMON --> PASTE_OBSERVER
    PASTE_OBSERVER --> CG_SERVER

    APP_ENTRY <-->|IPC Invoke / Send| PRELOAD_API
    PRELOAD_API <-->|contextBridge.exposeInMainWorld| RENDERER

    AUDIO_WORKLET -->|socket.emit('audio_chunk')| SOCKET_IO
    SOCKET_IO --> FAST_PIPELINE
    SOCKET_IO --> CLASSIC_PIPELINE
    FAST_PIPELINE --> GATE_KEEPER
    GATE_KEEPER -->|socket.emit('realtime_audio_delta')| PCM_QUEUE
    PCM_QUEUE -->|WebAudio API setSinkId| CORE_AUDIO
    HAL_DRIVER -->|System Audio Feed| AUDIO_WORKLET
```

---

# 2. Sơ Đồ 2: Chu Trình Xử Lý Âm Thanh Hai Luồng (Dual-Channel Audio Pipeline & Jitter Buffer)

Hệ thống bắt đồng thời 2 luồng âm thanh và xử lý lọc nhiễu, chuyển tần số, chống vọng âm trước khi chuyển tiếp cho AI:

```mermaid
flowchart LR
    subgraph INGESTION["1. THU ÂM & CHUYỂN TẦN SỐ MẪU (Sample Rate Adaptation)"]
        MIC_IN["Microphone Vật Lý (44.1k / 48kHz Float32)"] --> DOWNSAMPLE_MIC["StreamingPcmDownsampler (Continuous Box Filter)"]
        SYS_IN["Họp Zoom/Meet (BlackHole 48kHz Float32)"] --> DOWNSAMPLE_SYS["StreamingPcmDownsampler (Continuous Box Filter)"]
        DOWNSAMPLE_MIC -->|16kHz 16-bit PCM Mono| MERGE_GATE["Dual-Stream TTS Capture Suppression Gate"]
        DOWNSAMPLE_SYS -->|16kHz 16-bit PCM Mono| MERGE_GATE
    end

    subgraph ANTI_ECHO["2. LỌC TIẾNG VỌNG & KHỬ ÂM HỒI (Anti-Feedback Guard)"]
        TTS_STATUS{"Giọng đọc dịch AI đang phát?"}
        MERGE_GATE --> TTS_STATUS
        TTS_STATUS -->|ĐANG PHÁT| SUPPRESS_CHUNK["Thay thế bằng Silence Buffer (Ngăn Micro tự thu tiếng dịch)"]
        TTS_STATUS -->|KHÔNG PHÁT| PASS_CHUNK["Giữ nguyên Buffer âm thanh nguyên bản"]
    end

    subgraph RESAMPLER["3. NỘI SUY TUYẾN TÍNH (16k -> 24k)"]
        PASS_CHUNK --> LINEAR_MATH["pcmResampler.ts (Linear Interpolation ratio 1.5)"]
        LINEAR_MATH --> CLAMP["Clamp Int16 Bounds [-32768, 32767]"]
        CLAMP --> CHUNKER["100ms - 200ms Frame Commit (Min 4,800 bytes)"]
    end

    subgraph REALTIME_SOCKET["4. OPENAI REALTIME PROTOCOL"]
        CHUNKER --> WS_APPEND["input_audio_buffer.append (Base64 PCM24)"]
        WS_APPEND --> WS_COMMIT["input_audio_buffer.commit"]
    end

    subgraph JITTER_BUFFER["5. PHÁT ÂM THANH MƯỢT (PcmPlaybackQueue Jitter Buffer)"]
        S2S_OUT["response.audio.delta (Base64 PCM24 từ AI)"] --> HOLD_GATE["SameLanguageOutputGate (Giữ 500ms)"]
        HOLD_GATE -->|Phê duyệt phát| QUEUE["PcmPlaybackQueue (Underrun Grace 4ms, Tail 150ms)"]
        QUEUE --> DECODE["AudioContext.createBuffer(1, samples, 24000)"]
        DECODE --> PLAY["AudioBufferSourceNode.start(nextStartTime)"]
        PLAY --> SINK["Output Sink: Tai nghe (Incoming) HOẶC Micro ảo (Outgoing)"]
    end

    REALTIME_SOCKET --> S2S_OUT
```

---

# 3. Sơ Đồ 3: Máy Trạng Thái Giao Thức OpenAI Realtime WebSocket (State Machine)

Toàn bộ phiên làm việc Realtime S2S được điều khiển bởi máy trạng thái phức hợp xử lý kết nối, bắt tay, streaming và giải phóng:

```mermaid
stateDiagram-v2
    [*] --> Disconnected : Khởi động

    Disconnected --> Connecting : Bấm Start Recording
    note right of Connecting
        Mở WebSocket:
        wss://api.openai.com/v1/realtime/translations
        ?model=gpt-realtime-translate
        Header: Authorization: Bearer OPENAI_KEY
    end note

    Connecting --> ConfiguringSession : WebSocket Open
    ConfiguringSession --> SessionReady : Nhận 'session.created' & Gửi 'session.update'
    note right of ConfiguringSession
        Gửi payload:
        audio.output.language = targetLang
        audio.input.transcription.model = 'gpt-realtime-whisper'
        audio.input.noise_reduction.type = 'near_field' / 'far_field'
    end note

    state SessionReady {
        [*] --> Idle
        Idle --> AudioStreaming : Client gửi audio_chunk
        AudioStreaming --> AudioCommitted : Đạt mốc commit (>100ms PCM)
        AudioCommitted --> ResponseStreaming : OpenAI nhả token đầu tiên

        state ResponseStreaming {
            [*] --> ReceivingTranscript : response.audio_transcript.delta
            ReceivingTranscript --> ReceivingAudio : response.audio.delta (PCM24)
            ReceivingAudio --> EvaluatingLanguage : Đưa vào SameLanguageOutputGate
            EvaluatingLanguage --> Suppressed : Ngôn ngữ trùng (Same Language)
            EvaluatingLanguage --> Dispatched : Ngôn ngữ khác (Khác ngôn ngữ)
        }

        ResponseStreaming --> ResponseFinished : response.done
        ResponseFinished --> Idle : Chuẩn bị cho lượt thoại tiếp theo
    }

    SessionReady --> Reconnecting : Mạng ngắt đột ngột (Code 1006)
    Reconnecting --> SessionReady : Tái kết nối thành công (<2.5s)
    Reconnecting --> ClassicFallback : Quá thời gian chờ (Downgrade to Deepgram)

    SessionReady --> Closing : Bấm Stop Recording
    Closing --> Disconnected : Gửi input_audio_buffer.clear & ws.close()
```

---

# 4. Sơ Đồ 4: Kiến Trúc Phím Tắt Fn Toàn Hệ Thống (Native Swift EventTap & Accessibility)

Tính năng Voice Keyboard biến bất kỳ ô văn bản nào trên macOS thành giao diện nhận diện giọng nói mà không cần mở app:

```mermaid
sequenceDiagram
    autonumber
    actor User as Người Dùng
    participant OS as macOS WindowServer
    participant Swift as fn-key-listener.swift (Kernel Tap)
    participant TargetApp as App Đang Mở (VD: Slack / VS Code)
    participant Electron as Electron Main Process
    participant Whisper as Whisper / Nova-3 Engine
    participant LLM as Prompt Formatter (GPT-4o-mini)

    User->>OS: Nhấn giữ phím Fn (Key Code 63 / FlagsChanged)
    OS->>Swift: CGEventTapCallback (.flagsChanged)
    Swift->>Swift: Kiểm tra bitmask (flags.contains(.maskSecondaryFn))
    Swift->>OS: AXUIElementCreateSystemWide()
    Swift->>TargetApp: AXUIElementCopyAttributeValue(kAXFocusedUIElementAttribute)
    TargetApp-->>Swift: Trả về AXUIElement (Role: kAXTextAreaRole)
    Swift->>Electron: stdout.write("fn-down\tkAXTextAreaRole\n")
    Electron->>Electron: Bắt đầu thu luồng âm thanh PCM từ Micro

    User->>User: Nói khẩu lệnh: "Báo cáo tiến độ: xuống dòng gạch đầu dòng xong phần API chấm"

    User->>OS: Thả phím Fn
    OS->>Swift: CGEventTapCallback (.flagsChanged không còn cờ Fn)
    Swift->>Electron: stdout.write("fn-up\n")
    Electron->>Electron: Đóng buffer thu âm, gửi audio lên Whisper
    Whisper-->>Electron: "Báo cáo tiến độ hai chấm xuống dòng gạch đầu dòng xong phần API chấm"
    Electron->>LLM: Gửi DICTATION_FORMATTING_SYSTEM_PROMPT
    LLM-->>Electron: "Báo cáo tiến độ:\n- Xong phần API."
    Electron->>Swift: stdin.write("insert-text\tBáo cáo tiến độ:\n- Xong phần API.")

    alt Hỗ trợ ghi đè Accessibility trực tiếp
        Swift->>TargetApp: AXUIElementSetAttributeValue(element, kAXValueAttribute, formattedText)
    else Ứng dụng bảo mật / không cấp quyền AX (Terminal, App Java, Game)
        Swift->>Swift: Lưu clipboard tạm
        Swift->>OS: Gán formattedText vào NSPasteboard.general
        Swift->>TargetApp: CGEventPost(Cmd+V KeyDown) -> CGEventPost(Cmd+V KeyUp)
        Swift->>Swift: Khôi phục lại clipboard cũ của người dùng
    end
    TargetApp-->>User: Văn bản hoàn thiện xuất hiện ngay dưới con trỏ soạn thảo
```

---

# 5. Sơ Đồ 5: Hồ Phiên Đa Ngôn Ngữ Tiết Kiệm 94% Chi Phí (Language-Pooled Web Relay)

Bản vẽ giải thích thuật toán gom nhóm phiên theo ngôn ngữ đích giúp máy chủ chịu tải hàng trăm khách dự họp:

```mermaid
graph TD
    subgraph DESKTOP_HOST["PHÒNG HỌP GỐC (Host Desktop App)"]
        HOST_MIC["Giọng Host (tiếng Anh)"]
        HOST_SYS["Tiếng Đối Tác (tiếng Nhật)"]
    end

    subgraph POOL_MANAGER["HỒ PHIÊN ĐA NGÔN NGỮ (webGuestRealtimeSessionPool.ts)"]
        INGEST_ROUTER["Router Điều Phối Âm Thanh Socket.IO"]
        LANGUAGE_HASH_TABLE{"Bảng Băm Phiên Ngôn Ngữ (Map<LangKey, Session>)"}
    end

    subgraph ACTIVE_AI_SESSIONS["CÁC PHIÊN OPENAI ĐANG HOẠT ĐỘNG (Duy nhất 1 phiên / 1 ngôn ngữ)"]
        SESSION_VI["Phiên AI Dịch Tiếng Việt (gpt-realtime-translate)"]
        SESSION_JA["Phiên AI Dịch Tiếng Nhật (gpt-realtime-translate)"]
        SESSION_ES["Phiên AI Dịch Tiếng Tây Ban Nha (gpt-realtime-translate)"]
    end

    subgraph CLIENT_BROADCAST["BROADCAST HÀNG LOẠT ĐẾN KHÁCH WEB (Web Audio / Subtitles)"]
        GUEST_VI_1["Khách 1 (Xem tiếng Việt)"]
        GUEST_VI_2["Khách 2 (Xem tiếng Việt)"]
        GUEST_VI_N["Khách N (Xem tiếng Việt)"]

        GUEST_JA_1["Khách 1 (Xem tiếng Nhật)"]
        GUEST_JA_N["Khách N (Xem tiếng Nhật)"]

        GUEST_ES_1["Khách 1 (Xem tiếng Tây Ban Nha)"]
    end

    HOST_MIC --> INGEST_ROUTER
    HOST_SYS --> INGEST_ROUTER
    INGEST_ROUTER --> LANGUAGE_HASH_TABLE

    LANGUAGE_HASH_TABLE -->|Chỉ 1 Socket| SESSION_VI
    LANGUAGE_HASH_TABLE -->|Chỉ 1 Socket| SESSION_JA
    LANGUAGE_HASH_TABLE -->|Chỉ 1 Socket| SESSION_ES

    SESSION_VI -->|Phát sóng Socket.IO Room 'room-vi'| GUEST_VI_1
    SESSION_VI -->|Phát sóng Socket.IO Room 'room-vi'| GUEST_VI_2
    SESSION_VI -->|Phát sóng Socket.IO Room 'room-vi'| GUEST_VI_N

    SESSION_JA -->|Phát sóng Socket.IO Room 'room-ja'| GUEST_JA_1
    SESSION_JA -->|Phát sóng Socket.IO Room 'room-ja'| GUEST_JA_N

    SESSION_ES -->|Phát sóng Socket.IO Room 'room-es'| GUEST_ES_1
```

---

# 6. Sơ Đồ 6: Vòng Đời Tóm Tắt Tức Thì 3 Giây (Live 3s Bullet Point Life Cycle)

```mermaid
flowchart TD
    START(["Người nói phát biểu trong cuộc họp"]) --> BUFFER_APPEND["Đưa token phiên âm vào transcriptBuffer[]"]
    BUFFER_APPEND --> DECISION_BRANCH{"Điều kiện nào kích hoạt trước?"}

    DECISION_BRANCH -->|Đồng hồ 3 giây điểm hẹn| TIMER_TRIGGER["Timer 3000ms kích hoạt processTranscriptBuffer()"]
    DECISION_BRANCH -->|Người nói dứt câu ngưng thở| VAD_TRIGGER["Deepgram UtteranceEnd kích hoạt forceProcessTranscriptBuffer()"]

    TIMER_TRIGGER --> MERGE_TEXT["Gộp toàn bộ văn bản trong Buffer + 10 Bullet Points gần nhất"]
    VAD_TRIGGER --> CANCEL_TIMER["Hủy timer 3s đang chờ, chuyển sang xử lý cưỡng bức ngay"]
    CANCEL_TIMER --> MERGE_TEXT

    MERGE_TEXT --> PROMPT_INJECT["Tiêm Prompt Thẩm Định: reject từ đệm ('yeah', 'okay', 'right')"]
    PROMPT_INJECT --> OPENAI_MINI["Gọi OpenAI gpt-4o-mini (response_format: json_object)"]

    OPENAI_MINI --> EVAL_RESULT{"generate === true và có nội dung summary?"}
    EVAL_RESULT -->|FALSE: Chỉ là câu ậm ừ hoặc ý chưa trọn| KEEP_BUFFER["Giữ lại Buffer chờ câu nói tiếp theo hoàn chỉnh ý"]
    EVAL_RESULT -->|TRUE: Ý kiến có giá trị / Quyết định| PUSH_BULLET["Tạo đối tượng BulletPoint { text, timestamp, speaker }"]

    PUSH_BULLET --> SPLICE["transcriptBuffer.splice(0, chunksProcessed.length) - Xóa đoạn đã tóm tắt"]
    SPLICE --> EMIT_SOCKET["socket.emit('bullet_point', newBulletPoint)"]
    EMIT_SOCKET --> UI_RENDER["Giao diện hiển thị gạch đầu dòng mới kèm hoạt họa chuyển động mượt"]
```

---

# 7. Sơ Đồ 7: Pipeline Xử Lý File Kéo Thả 27 Định Dạng (Direct S3 Staging & Dubbing)

```mermaid
sequenceDiagram
    autonumber
    actor User as Người Dùng
    participant UI as TranscribeFileWorkspace.tsx
    participant Server as /api/transcribe Routes
    participant S3 as AWS S3 Storage
    participant Worker as Background Transcription & Dubbing Worker
    participant DB as Prisma / MongoDB

    User->>UI: Kéo thả file (PDF, DOCX, MP4, MP3, SRT) vào ứng dụng
    UI->>UI: HTML5 Video/Audio API đọc probeMediaDurationSeconds()
    UI->>Server: POST /api/transcribe/preflight { fileName, fileSize, durationSeconds }
    Server->>DB: Kiểm tra số dư quota (MonthlyMinutesUsage)
    DB-->>Server: Quota hợp lệ
    Server->>S3: getSignedUrlPromise('putObject', { Key: s3UploadKey, Expires: 900 })
    S3-->>Server: Trả về Pre-signed Upload URL
    Server-->>UI: { uploadUrl, stagingId }

    UI->>S3: XMLHttpRequest trực tiếp tải binary lên S3 (Client-to-S3 direct)
    S3-->>UI: Upload thành công 100% (HTTP 200 OK)

    UI->>Server: POST /api/transcribe/finalize { stagingId, targetLanguage, notesLanguages }
    Server->>DB: Tạo bản ghi FileTranslation / VideoDubbingJob { status: 'processing' }
    Server-->>UI: { meetingId: newRecordId }
    UI->>UI: Chuyển sang màn hình xem chi tiết (DesktopRecordingDetail.tsx)

    loop Worker định kỳ xử lý hàng đợi
        Worker->>DB: Nhận job 'processing'
        Worker->>S3: Tải file thô từ s3UploadKey
        alt Là File Audio / Video
            Worker->>Worker: Trích xuất audio PCM -> Gọi STT Deepgram Nova-3
            Worker->>Worker: Chạy kịch bản dịch thuật sang targetLanguage
            Worker->>Worker: Tùy chọn: Dùng ElevenLabs / Polly tạo giọng lồng tiếng mới
        else Là File Văn Bản (PDF / DOCX / PPTX)
            Worker->>Worker: Giữ nguyên cấu trúc XML/Trang -> Dịch text song song
        end
        Worker->>S3: Lưu kết quả vào resultKey
        Worker->>DB: Cập nhật status = 'completed', lưu transcript & meetingNotes
    end
```

---

# 8. Sơ Đồ 8: Sơ Đồ Thực Thể Cơ Sở Dữ Liệu Lõi (Prisma ERD Diagram)

Mô hình dữ liệu quan hệ trích xuất từ `server/prisma/schema.prisma` (98 KB):

```mermaid
erDiagram
    User ||--o{ Meeting : "created_by"
    User ||--o{ GoogleCalendarConnection : "has"
    User ||--o{ UserVocabulary : "manages"
    User ||--o{ OperationalContext : "defines"
    User ||--o{ FileTranslation : "owns"
    User ||--o{ VideoDubbingJob : "initiates"
    User ||--o{ Subscription : "subscribes"
    User ||--o{ MonthlyUsage : "tracks"

    Team ||--o{ TeamMembership : "includes"
    User ||--o{ TeamMembership : "belongs_to"

    Meeting ||--o{ BulletPoints : "has_live_bullets"
    Meeting ||--o{ MeetingComment : "has_comments"
    Meeting ||--o{ MeetingChat : "contains_copilot_qna"
    Meeting ||--o{ LiveTranslationInvitation : "generates_shares"

    Meeting {
        string id PK
        string createdById FK
        string title
        datetime timestamp
        datetime endTime
        string googleMeetingId
        json transcript
        string notepad
        string gist
        json meetingNotes
        string notesGenerationStatus
        int notesGenerationAttempts
        boolean isDeleted
    }

    LiveTranslationInvitation {
        string id PK
        string meetingId FK
        string shareCode UK
        string hostUserId
        string[] allowedLanguages
        datetime expiresAt
        boolean requireAttendeeEmail
    }

    BulletPoints {
        string id PK
        string meetingId FK
        string text
        datetime timestamp
        string speaker
        string explanation
    }

    UserVocabulary {
        string id PK
        string userId FK
        string term
        json translations
        string notes
        string category
    }

    OperationalContext {
        string id PK
        string userId FK
        string companyName
        string teamMission
        string userRole
        string contextDescription
        string[] industryKeywords
    }

    Subscription {
        string id PK
        string userId FK
        string stripeSubscriptionId UK
        string status
        string currentPlan
        datetime currentPeriodEnd
    }

    MonthlyUsage {
        string id PK
        string userId FK
        datetime monthYear
        int transcriptionMinutesUsage
        int transcriptionMinutesLimit
        int translationMinutesUsage
        int translationMinutesLimit
        int AICreditsUsage
        int AICreditsLimit
    }
```

---

# 9. Bảng Đặc Tả Toàn Bộ Hợp Đồng Sự Kiện IPC & WebSocket (Event Contracts)

### 9.1. Giao Tiếp Electron IPC (`main` $\longleftrightarrow$ `renderer`)

| Kênh IPC | Chiều | Kiểu Dữ Liệu | Mục Đích Kỹ Thuật |
| :--- | :--- | :--- | :--- |
| `getAudioRoutingCapabilities` | Renderer $\to$ Main | `invoke()` $\to$ `AudioRoutingCaps` | Lấy danh sách driver ảo cài đặt trên máy (`BlackHole`, `VB-CABLE`). |
| `openBundledBlackHoleInstaller`| Renderer $\to$ Main | `invoke(channelCount: '2ch'|'16ch')` | Mở trình cài đặt package `.pkg` của driver BlackHole đóng gói sẵn. |
| `window-view-mode` | Renderer $\to$ Main | `send(mode: 'main'|'recording')` | Báo Main process hoán đổi kích thước cửa sổ giữa Dashboard và HUD nổi. |
| `set-always-on-top` | Renderer $\to$ Main | `send(flag: boolean)` | Bật chế độ ghim cửa sổ nổi luôn ở trên cùng màn hình khi họp. |
| `fn-key-status` | Main $\to$ Renderer | `send(status: 'down'|'up')` | Thông báo phím Fn vừa được nhấn giữ hoặc thả ra để bật/tắt ghi âm dictation. |
| `learning-state` | Main $\to$ Renderer | `send(state: LearningState)` | Cập nhật tiến độ giải đố ngoại ngữ và trạng thái chặn mạng xã hội. |
| `dock-file-opened` | Main $\to$ Renderer | `send({ filePath, token })` | Bắn sự kiện khi người dùng kéo thả file tài liệu/video vào icon ứng dụng trên Dock. |

### 9.2. Giao Tiếp Socket.IO (`renderer` $\longleftrightarrow$ `server`)

| Tên Sự Kiện Socket | Chiều | Cấu Trúc Dữ Liệu | Mô Tả Nghiệp Vụ |
| :--- | :--- | :--- | :--- |
| `start_recording` | Client $\to$ Server | `{ meetingId, spokenLanguage, targetLanguage, translationMode, speechToSpeechMode }` | Khởi động phiên ghi âm và cấp phát luồng AI. |
| `audio_chunk` | Client $\to$ Server | `Binary Buffer (Int16 PCM 16kHz)` | Truyền các khối âm thanh nén thời gian thực lên server. |
| `pipeline_mode` | Server $\to$ Client | `{ mode: 'realtime-fast'|'classic', model }` | Báo cho Client biết đang chạy OpenAI WebSocket hay Deepgram. |
| `realtime_transcript_delta` | Server $\to$ Client | `{ delta, isFinal, speaker, desktopAudioSource, sectionId }` | Đẩy từng từ phiên âm và dịch thuật xuống hiển thị dòng chữ nhảy mượt. |
| `realtime_audio_delta` | Server $\to$ Client | `{ pcmBase64, sequence, elapsedMs }` | Đẩy từng gói âm thanh dịch S2S xuống cho hàng đợi `PcmPlaybackQueue`. |
| `bullet_point` | Server $\to$ Client | `{ text, timestamp, speaker }` | Đẩy gạch đầu dòng ý chính tự động sinh sau mỗi 3 giây hoặc dứt câu. |
| `meeting_notes_ready` | Server $\to$ Client | `{ gist, actionItems, keyPoints, otherPoints }` | Thông báo biên bản cuộc họp hoàn chỉnh sau phiên đã sinh xong. |
