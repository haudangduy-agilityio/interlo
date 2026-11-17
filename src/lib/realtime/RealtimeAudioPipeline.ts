import { OpenAiRealtimeClient } from './OpenAiRealtimeClient';
import { RealtimeClientConfig, RealtimeSocketConstructor } from './types';
import { PcmPlaybackQueue } from '../audio/PcmPlaybackQueue';
import { SameLanguageGate } from '../audio/SameLanguageGate';
import { StreamingPcmDownsampler } from '../audio/StreamingPcmDownsampler';

export interface RealtimeAudioPipelineCallbacks {
  onOriginalTranscript?: (text: string) => void;
  onTranslatedTranscript?: (delta: string, isFinal: boolean) => void;
  onAudioOutput?: (pcmChunk: Int16Array, durationMs: number) => void;
  onStateChange?: (state: string) => void;
  onError?: (err: Error) => void;
}

/** How long the gate holds output frames while the spoken language is being identified. */
const GATE_HOLD_MS = 500;

/**
 * Unified Realtime Speech-to-Speech Translation Pipeline.
 * Connects Audio Input Downsampling -> Realtime WebSocket -> Same-Language Gate -> Jitter Playback Buffer.
 */
export class RealtimeAudioPipeline {
  private client: OpenAiRealtimeClient;
  private playbackQueue: PcmPlaybackQueue;
  private languageGate: SameLanguageGate;
  private downsampler: StreamingPcmDownsampler;
  private callbacks: RealtimeAudioPipelineCallbacks;
  private sourceLanguage: string;
  private targetLanguage: string;
  private frameCounter = 0;
  private drainTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    config: RealtimeClientConfig & { inputSampleRate?: number },
    callbacks: RealtimeAudioPipelineCallbacks = {}
  ) {
    this.callbacks = callbacks;
    const outRate = config.sampleRate || 24000;
    this.playbackQueue = new PcmPlaybackQueue({ sampleRate: outRate });
    this.sourceLanguage = config.sourceLanguage || 'Auto';
    this.targetLanguage = config.targetLanguage;
    this.languageGate = this.createGate();

    // Default input downsampler (e.g. 48kHz input to 24kHz target, ratio 2:1)
    const inRate = config.inputSampleRate || 48000;
    this.downsampler = new StreamingPcmDownsampler({
      inputSampleRate: inRate,
      outputSampleRate: outRate
    });

    this.client = new OpenAiRealtimeClient(config, {
      onStateChange: (state) => this.callbacks.onStateChange?.(state),
      onUserTranscript: (text) => this.callbacks.onOriginalTranscript?.(text),
      onTranscriptDelta: (delta, isFinal) =>
        this.callbacks.onTranslatedTranscript?.(delta, isFinal),
      onAudioDelta: (pcmChunk) => this.handleIncomingAudio(pcmChunk),
      onInputSpeechStarted: () => this.handleInterruption(),
      onError: (err) => this.callbacks.onError?.(err)
    });
  }

  /**
   * Build a gate for the current language pair. A known (non-auto) source language
   * is registered up front so same-language audio is suppressed.
   */
  private createGate(): SameLanguageGate {
    const gate = new SameLanguageGate(this.targetLanguage, GATE_HOLD_MS);
    if (this.sourceLanguage.toLowerCase() !== 'auto') {
      gate.setDetectedLanguage(this.sourceLanguage);
    }
    return gate;
  }

  private now(): number {
    return performance.now() / 1000;
  }

  public updateLanguages(sourceLang: string, targetLang: string) {
    this.sourceLanguage = sourceLang;
    this.targetLanguage = targetLang;
    this.resetOutput();
  }

  public async start(customWsConstructor?: RealtimeSocketConstructor): Promise<void> {
    await this.client.connect(customWsConstructor);
  }

  public stop(): void {
    this.client.disconnect();
    this.resetOutput();
  }

  /**
   * Feed raw Float32 microphone or system loopback audio into the pipeline
   */
  public pushInputAudio(float32Chunk: Float32Array): void {
    const pcm16 = this.downsampler.process(float32Chunk);
    if (pcm16.length > 0) {
      this.client.appendAudioChunk(pcm16);
    }
  }

  /**
   * Hold incoming PCM audio in the language gate, then schedule released frames in the playback queue
   */
  private handleIncomingAudio(pcmChunk: Int16Array): void {
    if (pcmChunk.length === 0) {
      return;
    }

    this.languageGate.enqueue(`frame-${this.frameCounter++}`, pcmChunk);
    this.drainGate();
    this.scheduleDrain();
  }

  /**
   * Release frames that passed the gate hold time and schedule them for playback.
   */
  private drainGate(): void {
    for (const frame of this.languageGate.releaseReadyFrames()) {
      const scheduled = this.playbackQueue.scheduleChunk(frame, this.now());
      this.callbacks.onAudioOutput?.(frame, scheduled.duration * 1000);
    }
  }

  /**
   * Make sure frames still held by the gate are released once their hold time expires,
   * even if no further audio arrives.
   */
  private scheduleDrain(): void {
    if (this.drainTimer !== null) {
      return;
    }
    const delayMs = this.languageGate.getNextReleaseDelayMs();
    if (delayMs === null) {
      return;
    }
    this.drainTimer = setTimeout(() => {
      this.drainTimer = null;
      this.drainGate();
      this.scheduleDrain();
    }, delayMs);
  }

  /**
   * Drop all pending and held output audio and restore the gate for the current language pair.
   */
  private resetOutput(): void {
    if (this.drainTimer !== null) {
      clearTimeout(this.drainTimer);
      this.drainTimer = null;
    }
    this.playbackQueue.flush();
    this.languageGate = this.createGate();
  }

  /**
   * User interrupted during playback: clear queue and signal client
   */
  private handleInterruption(): void {
    this.resetOutput();
    this.client.interrupt();
  }

  public getQueueStats() {
    return {
      isPlaying: this.playbackQueue.isBusy(this.now()),
      queuedChunks: this.languageGate.getQueuedFrameCount()
    };
  }
}
