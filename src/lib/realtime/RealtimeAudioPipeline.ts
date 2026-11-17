import { OpenAiRealtimeClient } from './OpenAiRealtimeClient';
import { RealtimeClientConfig } from './types';
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

  constructor(
    config: RealtimeClientConfig & { inputSampleRate?: number },
    callbacks: RealtimeAudioPipelineCallbacks = {}
  ) {
    this.callbacks = callbacks;
    this.playbackQueue = new PcmPlaybackQueue(config.sampleRate || 24000);
    this.languageGate = new SameLanguageGate(
      config.sourceLanguage || 'Auto',
      config.targetLanguage
    );

    // Default input downsampler (e.g. 48kHz input to 24kHz target, ratio 2:1)
    const inRate = config.inputSampleRate || 48000;
    const outRate = config.sampleRate || 24000;
    this.downsampler = new StreamingPcmDownsampler(inRate, outRate);

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

  public updateLanguages(sourceLang: string, targetLang: string) {
    this.languageGate.setLanguages(sourceLang, targetLang);
  }

  public async start(customWsConstructor?: any): Promise<void> {
    await this.client.connect(customWsConstructor);
  }

  public stop(): void {
    this.client.disconnect();
    this.playbackQueue.clear();
    this.languageGate.reset();
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
   * Process incoming PCM audio chunk through language gate and schedule in playback queue
   */
  private handleIncomingAudio(pcmChunk: Int16Array): void {
    const filteredChunk = this.languageGate.process(pcmChunk);
    if (!filteredChunk || filteredChunk.length === 0) {
      return;
    }

    const scheduled = this.playbackQueue.enqueue(filteredChunk);
    this.callbacks.onAudioOutput?.(scheduled.chunk, scheduled.durationMs);
  }

  /**
   * User interrupted during playback: clear queue and signal client
   */
  private handleInterruption(): void {
    this.playbackQueue.clear();
    this.client.interrupt();
  }

  public getQueueStats() {
    return {
      isPlaying: this.playbackQueue.isPlaying(),
      queuedChunks: this.playbackQueue.getQueueLength()
    };
  }
}
