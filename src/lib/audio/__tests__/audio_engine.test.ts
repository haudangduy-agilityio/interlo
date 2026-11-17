import { describe, expect, it } from "bun:test";
import { StreamingPcmDownsampler } from '../StreamingPcmDownsampler';
import { LinearPcmResampler } from '../LinearPcmResampler';
import { PcmPlaybackQueue } from '../PcmPlaybackQueue';
import { SameLanguageGate } from '../SameLanguageGate';

describe('Audio Engine DSP & Queue Tests', () => {
  describe('StreamingPcmDownsampler', () => {
    it('downsamples 48kHz Float32 to 16kHz Int16 (3:1 ratio)', () => {
      const downsampler = new StreamingPcmDownsampler({ inputSampleRate: 48000, outputSampleRate: 16000 });
      // 480 samples at 48kHz should produce 160 samples at 16kHz
      const input = new Float32Array(480);
      for (let i = 0; i < input.length; i++) {
        input[i] = Math.sin((i / 480) * 2 * Math.PI); // 1 period of sine
      }

      const output = downsampler.process(input);
      expect(output.length).toBe(160);
      expect(output instanceof Int16Array).toBe(true);
    });

    it('clamps audio values within Int16 range (-32768 to 32767)', () => {
      const downsampler = new StreamingPcmDownsampler({ inputSampleRate: 48000, outputSampleRate: 16000 });
      const input = new Float32Array([1.5, -1.5, 0.5, 0.0, 1.0, -1.0]);
      const output = downsampler.process(input);

      for (let i = 0; i < output.length; i++) {
        expect(output[i]).toBeGreaterThanOrEqual(-32768);
        expect(output[i]).toBeLessThanOrEqual(32767);
      }
    });
  });

  describe('LinearPcmResampler', () => {
    it('resamples 16kHz Int16 to 24kHz Int16 (2:3 ratio)', () => {
      const input = new Int16Array(160);
      for (let i = 0; i < input.length; i++) {
        input[i] = i * 100;
      }

      const output = LinearPcmResampler.resample16kTo24k(input);
      expect(output.length).toBe(240);
    });

    it('resamples 24kHz Int16 back to 16kHz Int16', () => {
      const input = new Int16Array(240);
      const output = LinearPcmResampler.resample24kTo16k(input);
      expect(output.length).toBe(160);
    });
  });

  describe('PcmPlaybackQueue', () => {
    it('schedules non-overlapping chunks with grace time', () => {
      const queue = new PcmPlaybackQueue({ sampleRate: 24000, underrunGraceMs: 4 });
      const chunk1 = new Int16Array(2400); // 100ms at 24kHz
      const chunk2 = new Int16Array(2400); // 100ms at 24kHz

      const schedule1 = queue.scheduleChunk(chunk1, 1.0);
      expect(schedule1.scheduledTime).toBeCloseTo(1.004, 3);
      expect(schedule1.duration).toBeCloseTo(0.1, 3);

      const schedule2 = queue.scheduleChunk(chunk2, 1.05);
      expect(schedule2.scheduledTime).toBeCloseTo(1.104, 3);
      expect(schedule2.duration).toBeCloseTo(0.1, 3);
    });
  });

  describe('SameLanguageGate', () => {
    it('suppresses audio when source matches target language', () => {
      const gate = new SameLanguageGate('ja', 500);
      gate.enqueue('frame-1', new Int16Array(100));
      gate.enqueue('frame-2', new Int16Array(100));

      const result = gate.setDetectedLanguage('ja');
      expect(result.suppressed).toBe(true);
      expect(result.purgedFrames).toBe(2);

      const ready = gate.releaseReadyFrames(Date.now() + 600);
      expect(ready.length).toBe(0);
    });

    it('releases held frames after 500ms when language is different', () => {
      const gate = new SameLanguageGate('en', 500);
      const now = Date.now();
      gate.enqueue('frame-1', new Int16Array(100));

      gate.setDetectedLanguage('ja');
      // Before 500ms
      expect(gate.releaseReadyFrames(now + 200).length).toBe(0);
      // After 500ms
      const released = gate.releaseReadyFrames(now + 550);
      expect(released.length).toBe(1);
    });
  });
});
