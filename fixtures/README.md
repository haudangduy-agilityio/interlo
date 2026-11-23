# Fixtures

Fixtures are text only: no audio, no model files. A fixture made from a person's voice needs that person's written consent, recorded in this file.
`fixtures/ipc` holds the IPC contract payloads; the Rust and the UI test suites both read it.
`fixtures/audio` holds one sample of the output of the system audio tool (`system_profiler SPAudioDataType -json`) with generic device names; the `audio-io` tests read it.
