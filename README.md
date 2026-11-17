# Interlo

Real-time AI live translation, meeting transcription, and automated notes for the desktop.

## Structure

```text
src/
  components/interlo/   UI components
  lib/audio/            Audio DSP (downsampling, resampling, jitter buffer, same-language gate)
  lib/realtime/         Realtime speech-to-speech client and audio pipeline
  lib/classic/          Classic (non-S2S) fallback pipeline
```

## Tests

```bash
bun test src/lib/audio/__tests__/
bun test src/lib/realtime/__tests__/
```
