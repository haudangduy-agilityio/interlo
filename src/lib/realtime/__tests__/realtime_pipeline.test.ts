import { beforeEach, describe, expect, test } from 'bun:test';
import { RealtimeAudioPipeline } from '../RealtimeAudioPipeline';
import { pcm16ToBase64 } from '../OpenAiRealtimeClient';
import { RealtimeSocket } from '../types';

class MockWebSocket implements RealtimeSocket {
  public static instances: MockWebSocket[] = [];

  public static latest(): MockWebSocket {
    const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    if (!ws) {
      throw new Error('No MockWebSocket was created');
    }
    return ws;
  }

  public readyState = 0;
  public onopen: ((event: Event) => void) | null = null;
  public onmessage: ((event: MessageEvent) => void) | null = null;
  public onerror: ((event: Event) => void) | null = null;
  public onclose: ((event: CloseEvent) => void) | null = null;

  constructor(
    public url: string,
    public protocols: string[] = []
  ) {
    MockWebSocket.instances.push(this);
    setTimeout(() => {
      this.readyState = 1;
      this.onopen?.(new Event('open'));
    }, 1);
  }

  public send(): void {}

  public close(): void {
    this.readyState = 3;
    this.onclose?.(new CloseEvent('close'));
  }

  public receiveAudioDelta(chunk: Int16Array): void {
    const data = JSON.stringify({ type: 'response.audio.delta', delta: pcm16ToBase64(chunk) });
    this.onmessage?.(new MessageEvent('message', { data }));
  }
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe('RealtimeAudioPipeline gate drain', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
  });

  test('releases each held frame after its own hold time, not one full hold per batch', async () => {
    const emittedAt: number[] = [];
    const pipeline = new RealtimeAudioPipeline(
      {
        apiKey: 'test-key',
        sourceLanguage: 'Spanish',
        targetLanguage: 'English'
      },
      { onAudioOutput: () => emittedAt.push(Date.now()) }
    );
    await pipeline.start(MockWebSocket);
    await sleep(10);

    const chunk = new Int16Array(2400).fill(100);
    const start = Date.now();
    MockWebSocket.latest().receiveAudioDelta(chunk);
    await sleep(100);
    MockWebSocket.latest().receiveAudioDelta(chunk);
    await sleep(100);
    MockWebSocket.latest().receiveAudioDelta(chunk);

    // Frames were enqueued at ~0, ~100 and ~200 ms with a 500 ms hold, so all three must be
    // out by ~700 ms. A drain timer that restarts a full hold per batch would emit the last
    // two only at ~1000 ms.
    await sleep(700);
    expect(emittedAt.length).toBe(3);
    expect(emittedAt[0]! - start).toBeGreaterThanOrEqual(490);
    expect(emittedAt[2]! - start).toBeLessThan(900);

    pipeline.stop();
  });

  test('suppresses audio when source and target language match', async () => {
    const emitted: Int16Array[] = [];
    const pipeline = new RealtimeAudioPipeline(
      { apiKey: 'test-key', sourceLanguage: 'en', targetLanguage: 'en-US' },
      { onAudioOutput: (frame) => emitted.push(frame) }
    );
    await pipeline.start(MockWebSocket);
    await sleep(10);

    MockWebSocket.latest().receiveAudioDelta(new Int16Array(2400).fill(100));
    await sleep(600);

    expect(emitted.length).toBe(0);
    expect(pipeline.getQueueStats().queuedChunks).toBe(0);

    pipeline.stop();
  });
});
