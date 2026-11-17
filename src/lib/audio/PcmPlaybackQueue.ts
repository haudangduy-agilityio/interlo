/**
 * PcmPlaybackQueue
 * 
 * Jitter Buffer & Scheduling Queue for streaming 24kHz Int16 PCM audio playback.
 * Compensates for network jitter, WebSocket packet burstiness, and audio underrun.
 * 
 * Parameters unraveled from reverse engineering:
 * - UNDERRUN_GRACE_MS: 4ms
 * - PLAYBACK_TAIL_MS: 150ms
 */

export interface PlaybackQueueConfig {
  sampleRate?: number; // default: 24000
  underrunGraceMs?: number; // default: 4ms
  playbackTailMs?: number; // default: 150ms
}

export class PcmPlaybackQueue {
  private readonly sampleRate: number;
  private readonly underrunGraceMs: number;
  private readonly playbackTailMs: number;
  private nextPlayTime = 0;
  private isPlaying = false;
  private queue: Float32Array[] = [];

  constructor(config: PlaybackQueueConfig = {}) {
    this.sampleRate = config.sampleRate ?? 24000;
    this.underrunGraceMs = config.underrunGraceMs ?? 4;
    this.playbackTailMs = config.playbackTailMs ?? 150;
  }

  /**
   * Convert Int16 PCM chunk to Float32 array (-1.0 to 1.0).
   */
  public static int16ToFloat32(pcmChunk: Int16Array): Float32Array {
    const float32 = new Float32Array(pcmChunk.length);
    for (let i = 0; i < pcmChunk.length; i++) {
      float32[i] = (pcmChunk[i] || 0) / 32768;
    }
    return float32;
  }

  /**
   * Enqueue a new Int16 PCM delta from the streaming S2S pipeline.
   * Returns scheduling time for the AudioContext.
   */
  public scheduleChunk(
    chunk: Int16Array,
    currentAudioTime: number
  ): { scheduledTime: number; duration: number } {
    if (chunk.length === 0) {
      return { scheduledTime: currentAudioTime, duration: 0 };
    }

    const duration = chunk.length / this.sampleRate;
    const graceSeconds = this.underrunGraceMs / 1000;

    // If buffer starved or first chunk, reset baseline
    if (this.nextPlayTime < currentAudioTime + graceSeconds) {
      this.nextPlayTime = currentAudioTime + graceSeconds;
    }

    const scheduledTime = this.nextPlayTime;
    this.nextPlayTime += duration;

    return { scheduledTime, duration };
  }

  /**
   * Clear queue and reset timing (e.g. on user mute or speaker interruption).
   */
  public flush(): void {
    this.queue = [];
    this.nextPlayTime = 0;
    this.isPlaying = false;
  }

  /**
   * Check if playback queue has remaining audio.
   */
  public isBusy(currentAudioTime: number): boolean {
    return this.nextPlayTime > currentAudioTime;
  }
}
