# M0-01: Hardware and software baseline

Recorded 2026-11-16 on the owner's machine.

| Item | Value | Source |
|---|---|---|
| Model | Mac16,11 (Mac mini, Apple M4 Pro) | `sysctl -n hw.model` |
| CPU | Apple M4 Pro, 12 cores | `sysctl -n machdep.cpu.brand_string`, `hw.ncpu` |
| Memory | 24 GB (25769803776 bytes) | `sysctl -n hw.memsize` |
| macOS | 26.4.1 (build 25E253) | `sw_vers` |
| Disk, data volume | 54 GiB free of 460 GiB (88 percent used) | `df -h "$HOME"` |
| Virtual audio devices (2 channels, 16 channels) | not installed (0 devices found) | `system_profiler SPAudioDataType` |
| Power source | AC power, `powermode 0`. No Low Power Mode line reported | `pmset -g batt`, `pmset -g` |
| Rust | rustc 1.99.1 (58cf8f9c3, 2026-10-13); cargo 1.99.1 (5f94df478) | `rustc --version`, `cargo --version` |
| Node | v26.5.0; npm 11.17.0 | `node -v`, `npm -v` |
| Python | 3.14.7 (system `python3`) | `python3 --version` |
| Homebrew | 7.0.13 | `brew --version` |

Not yet recorded (needs the owner or a later step): headset models (H-09). Re-check the power source before each long measurement run (M0-03 onward) and note it in that run's record.
