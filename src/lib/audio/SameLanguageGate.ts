/**
 * SameLanguageGate
 * 
 * Sliding-window output gate holding audio frames for 500ms
 * while waiting for speech language identification.
 * If the spoken language matches the target language, audio is
 * suppressed to prevent echoing native speakers.
 */

export interface QueuedAudioFrame {
  id: string;
  chunk: Int16Array;
  timestamp: number;
}

export class SameLanguageGate {
  private readonly holdDurationMs: number;
  private queue: QueuedAudioFrame[] = [];
  private sourceLanguage: string | null = null;
  private targetLanguage: string;
  private isSuppressed = false;

  constructor(targetLanguage: string, holdDurationMs = 500) {
    this.targetLanguage = targetLanguage.toLowerCase().split('-')[0] || 'en';
    this.holdDurationMs = holdDurationMs;
  }

  /**
   * Enqueue a new raw audio frame.
   */
  public enqueue(id: string, chunk: Int16Array): void {
    if (this.isSuppressed) return;

    this.queue.push({
      id,
      chunk,
      timestamp: Date.now(),
    });
  }

  /**
   * Set or update detected source language.
   * If source matches target, purge all held frames and suppress future audio.
   */
  public setDetectedLanguage(language: string): { suppressed: boolean; purgedFrames: number } {
    const normalized = language.toLowerCase().split('-')[0] || '';
    this.sourceLanguage = normalized;

    if (normalized === this.targetLanguage) {
      const purged = this.queue.length;
      this.isSuppressed = true;
      this.queue = [];
      return { suppressed: true, purgedFrames: purged };
    }

    this.isSuppressed = false;
    return { suppressed: false, purgedFrames: 0 };
  }

  /**
   * Dequeue frames that have passed the 500ms hold time and are confirmed safe to play.
   */
  public releaseReadyFrames(now = Date.now()): Int16Array[] {
    if (this.isSuppressed) return [];

    const ready: Int16Array[] = [];
    const remaining: QueuedAudioFrame[] = [];

    for (const frame of this.queue) {
      if (now - frame.timestamp >= this.holdDurationMs) {
        ready.push(frame.chunk);
      } else {
        remaining.push(frame);
      }
    }

    this.queue = remaining;
    return ready;
  }

  /**
   * Number of frames currently held in the sliding window.
   */
  public getQueuedFrameCount(): number {
    return this.queue.length;
  }

  /**
   * Milliseconds until the oldest held frame passes the hold time (0 if it is already ready).
   * Returns null when no frames are held.
   */
  public getNextReleaseDelayMs(now = Date.now()): number | null {
    const oldest = this.queue[0];
    if (!oldest) return null;
    return Math.max(0, oldest.timestamp + this.holdDurationMs - now);
  }

  /**
   * Reset gate for new speaker utterance.
   */
  public reset(): void {
    this.queue = [];
    this.sourceLanguage = null;
    this.isSuppressed = false;
  }
}
