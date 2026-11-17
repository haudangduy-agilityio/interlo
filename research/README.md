# Master Research & Reverse Engineering Hub
## LiveMeetingCopilot Architecture, System Integration & Source Deconstruction

Thư mục này chứa toàn bộ tài nguyên, tài liệu phân tích kỹ thuật, sơ đồ kiến trúc, mã nguồn linh kiện giao diện (components), trang mẫu (pages), và mã nguồn thô bóc tách từ ứng dụng máy tính.

---

## 1. MỤC LỤC TÀI LIỆU NGHIÊN CỨU CHUYÊN SÂU

| STT | Tài Liệu | Nội Dung Kỹ Thuật Chính |
| :---: | :--- | :--- |
| **01** | [**FINDINGS.md**](./FINDINGS.md) | **Sổ cái sự thật & bằng chứng (Verified Findings & Evidence)**: 3 giỏ (`[CONFIRMED FACT]`, `[INFERENCE]`, `[UNKNOWN / GAP]`), Evidence ID `ev_9b1b8eb3fba4...`, thông số binary ARM64 Mach-O và runtime snapshot. |
| **02** | [**SPEC.md**](./SPEC.md) | **Đặc tả kỹ thuật hệ thống (Engineering Specification)**: Hướng dẫn kỹ thuật từng bước để xây dựng hệ thống bằng Pure TypeScript / React 19 / Node.js. |
| **03** | [**DEEP_ARCHITECTURE_ANALYSIS.md**](./DEEP_ARCHITECTURE_ANALYSIS.md) | **Phân tích mô hình toán học**: Bộ lọc DSP Continuous Box Filter, Jitter Buffer, Session Pooling $O(L)$ cho khách xem trực tiếp (tiết kiệm 94% chi phí API), và Billing Multiplier. |
| **04** | [**MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md**](./MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md) | **Sổ tay 12 cụm tính năng**: Bóc tách chi tiết từng tính năng kèm 4 sơ đồ tuần tự (Sequence Diagrams) cho S2S Fast Mode, Classic Mode, Real-time 3s Bullets, và Ask Copilot. |
| **05** | [**ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md**](./ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md) | **8 sơ đồ kiến trúc hệ thống**: FSM máy trạng thái Socket.IO, cơ chế Jitter Buffer, Prisma ERD, bảng hợp đồng IPC Electron và WebSocket events. |
| **06** | [**OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md**](./OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md) | **Tích hợp sâu hệ điều hành**: Virtual Audio Driver (BlackHole 2ch/16ch, VB-CABLE), bắt âm thanh loopback ngầm, JXA/Win32 UI Automation, phím Fn Dictation, và thuật toán chống rò rỉ prompt (`isLikelyVocabularyPromptLeak`). |
| **07** | [**CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md**](./CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md) | **Ma trận so sánh 3 nền tảng**: Phân tích sự hiệp đồng và chiến lược kỹ thuật giữa **Desktop App** (`device=desktop`), **Chrome Extension** (`device=chrome-extension`), và **Mobile App** (`device=mobile`). |

---

## 2. CẤU TRÚC THƯ MỤC TÀI NGUYÊN

```
research/
├── README.md                                          # Mục lục tổng quan và hướng dẫn tra cứu
├── FINDINGS.md                                        # Sổ cái sự thật & bằng chứng (3 giỏ Fact/Inference/Gap)
├── SPEC.md                                            # Đặc tả kỹ thuật hệ thống (Engineering Specification)
├── DEEP_ARCHITECTURE_ANALYSIS.md                      # Mô hình toán học & phân tích hệ thống
├── MASTER_FEATURE_AND_ALGORITHM_DECONSTRUCTION.md     # Bóc tách 12 tính năng cốt lõi
├── ARCHITECTURE_DIAGRAMS_AND_DETAILED_BREAKDOWN.md    # 8 sơ đồ kiến trúc & FSM
├── OS_INTEGRATION_AND_DEEP_ENGINEERING_ANALYSIS.md    # Tích hợp OS, Driver ảo & Tự động hóa
├── CROSS_PLATFORM_ECOSYSTEM_ANALYSIS.md               # So sánh Desktop vs Extension vs Mobile
├── components/                                        # 16 UI Components & Swift Helper đã khử bản quyền
│   ├── AudioRecorder.tsx                              # Bắt âm thanh 2 luồng & streaming
│   ├── Controllers.tsx                                # Thanh điều khiển nổi (Floating HUD pill)
│   ├── Transcript.tsx                                 # Luồng phụ đề song ngữ thời gian thực
│   ├── Bullet.tsx                                     # Tóm tắt ý chính mỗi 3 giây
│   ├── Ask.tsx                                        # Trợ lý AI hỏi đáp trong cuộc họp
│   ├── NotionEditor.tsx                               # Trình soạn thảo ghi chú rich-text HTML
│   ├── LanguageSettingsPill.tsx                       # Lựa chọn ngôn ngữ nguồn/đích
│   ├── TranslationLatencyMonitor.tsx                  # Giám sát độ trễ theo thời gian thực (ms)
│   ├── RealtimeTranslatedSpeechPlayer.tsx             # Phát giọng nói dịch Int16 PCM 24kHz
│   ├── DesktopRecordingDetail.tsx                     # Xem chi tiết cuộc họp & sóng âm WaveSurfer
│   ├── DesktopVoiceTyping.tsx                         # Cấu hình gõ giọng nói toàn hệ điều hành
│   ├── DesktopChat.tsx                                # Chat lịch sử cuộc họp
│   ├── DesktopHome.tsx                                # Màn hình chính Dashboard
│   ├── DesktopSidebar.tsx                             # Thanh điều hướng thu gọn
│   ├── DesktopShell.tsx                               # Khung cửa sổ chính Electron
│   └── fn-key-listener.swift                          # Helper Swift bắt phím Fn toàn hệ thống
├── pages/                                             # Các trang giao diện chính
│   ├── home.tsx                                       # Trang chủ / Khởi động
│   ├── dashboard.tsx                                  # Bảng điều khiển trung tâm
│   └── sign-up.tsx                                    # Trang đăng nhập / Đăng ký
└── raw/                                               # Liên kết đến 1.680 file mã nguồn thô trích xuất
    ├── package.json                                   # File cấu hình dependencies gốc
    ├── asar_filelist.txt                              # Danh mục toàn bộ 10.996 file trong asar
    └── extracted/                                     # Toàn bộ mã nguồn đã lọc bỏ node_modules
        ├── main/                                      # Electron Main process (TS & Swift)
        ├── renderer/                                  # Next.js / React Frontend
        ├── server/                                    # Express + Socket.IO Backend
        └── shared/                                    # Shared types, learning engines & vocabularies
```

---

## 3. CÁC ĐIỂM SÁNG KỸ THUẬT NỔI BẬT

1. **Phiên Dịch Giọng Nói Hai Chiều Không Tiếng Vọng (Acoustic Feedback Isolation)**:
   - Tách biệt 2 driver âm thanh ảo độc lập (`BlackHole 2ch` cho cuộc họp, `BlackHole 16ch` cho micro người dùng trên macOS; `VB-CABLE-A` và `VB-CABLE` trên Windows).
2. **Thu Âm Hệ Thống Chạy Ngầm Không Hộp Thoại (Silent Loopback Capture)**:
   - Hook `ses.setDisplayMediaRequestHandler` với `audio: "loopback", video: null` trong Electron Main process.
3. **Voice Keyboard Toàn Cầu (Fn Key Dictation)**:
   - Swift bắt sự kiện `CGEventTap` phím Fn (`0x800000`). Bắt đầu phiên kết nối ngay khi phím vừa chạm đáy, dán văn bản tự động qua Accessibility API và tự hủy khi trỏ vào ô mật khẩu.
4. **Learning Guard (Chống Sao Nhãng & Ôn Tập Từ Vựng Cuộc Họp)**:
   - Dùng JXA / UIAutomation theo dõi URL trình duyệt. Khi người dùng lướt mạng xã hội quá giới hạn, tự động đóng tab và hiển thị bài kiểm tra từ vựng 5 cấp độ sinh ra từ chính các cuộc họp trước đó.
5. **Chống Lộ Prompt STT Khi Im Lặng (`isLikelyVocabularyPromptLeak`)**:
   - Thuật toán loại trừ ảo giác nhại lại prompt của mô hình Whisper / OpenAI Realtime khi micro gặp khoảng lặng.
6. **Chiến Lược Đa Nền Tảng (3-Surface Strategy)**:
   - Chrome Extension làm phễu tiếp cận miễn phí không giới hạn trên Google Meet; Desktop App là cỗ máy tạo doanh thu toàn hệ điều hành; Mobile App là trợ lý gặp mặt trực tiếp ngoài đời thực (`in-person` mode) không cần cài driver âm thanh.
