import { describe, expect, test } from 'bun:test';
import {
  OpenAiRealtimeClient,
  pcm16ToBase64,
  base64ToPcm16
} from '../OpenAiRealtimeClient';
import { RealtimeClientState } from '../types';

/**
 * Mock WebSocket implementation for unit testing
 */
class MockWebSocket {
  public url: string;
  public protocols: string[];
  public readyState: number = 0; // 0: CONNECTING, 1: OPEN, 2: CLOSING, 3: CLOSED
  public sentMessages: string[] = [];

  public onopen: (() => void) | null = null;
  public onmessage: ((event: { data: string }) => void) | null = null;
  public onerror: ((event: any) => void) | null = null;
  public onclose: (() => void) | null = null;

  constructor(url: string, protocols: string[] = []) {
    this.url = url;
    this.protocols = protocols;
    setTimeout(() => {
      this.readyState = 1;
      this.onopen?.();
    }, 5);
  }

  public send(data: string) {
    this.sentMessages.push(data);
  }

  public close() {
    this.readyState = 3;
    this.onclose?.();
  }

  // Helper to simulate incoming server message
  public mockReceiveMessage(obj: any) {
    this.onmessage?.({ data: JSON.stringify(obj) });
  }
}

describe('OpenAiRealtimeClient', () => {
  test('PCM16 Base64 encoding and decoding lossless roundtrip', () => {
    const original = new Int16Array([0, 1000, -1000, 32767, -32768, 42]);
    const b64 = pcm16ToBase64(original);
    expect(typeof b64).toBe('string');
    expect(b64.length).toBeGreaterThan(0);

    const decoded = base64ToPcm16(b64);
    expect(decoded.length).toBe(original.length);
    for (let i = 0; i < original.length; i++) {
      expect(decoded[i]).toBe(original[i]);
    }
  });

  test('Connects, transitions state, and sends session.update with translation instructions', async () => {
    const states: RealtimeClientState[] = [];
    const client = new OpenAiRealtimeClient(
      {
        apiKey: 'test-key-xyz',
        sourceLanguage: 'English',
        targetLanguage: 'Vietnamese',
        voice: 'shimmer'
      },
      {
        onStateChange: (state) => states.push(state)
      }
    );

    let activeMockWs: MockWebSocket | null = null;
    const MockWsFactory = class extends MockWebSocket {
      constructor(url: string, protocols: string[] = []) {
        super(url, protocols);
        activeMockWs = this;
      }
    };

    await client.connect(MockWsFactory as any);

    // Wait for mock connection open
    await new Promise((r) => setTimeout(r, 20));

    expect(client.getState()).toBe('connected');
    expect(states).toContain('connecting');
    expect(states).toContain('connected');

    expect(activeMockWs).not.toBeNull();
    expect(activeMockWs!.sentMessages.length).toBeGreaterThan(0);

    const firstMsg = JSON.parse(activeMockWs!.sentMessages[0]);
    expect(firstMsg.type).toBe('session.update');
    expect(firstMsg.session.voice).toBe('shimmer');
    expect(firstMsg.session.instructions).toContain('English');
    expect(firstMsg.session.instructions).toContain('Vietnamese');

    client.disconnect();
    expect(client.getState()).toBe('disconnected');
  });

  test('Appends PCM audio buffer chunk formatted as base64', async () => {
    let activeMockWs: MockWebSocket | null = null;
    const client = new OpenAiRealtimeClient(
      { targetLanguage: 'Spanish' },
      {}
    );

    const MockWsFactory = class extends MockWebSocket {
      constructor(url: string, protocols: string[] = []) {
        super(url, protocols);
        activeMockWs = this;
      }
    };

    await client.connect(MockWsFactory as any);
    await new Promise((r) => setTimeout(r, 15));

    const chunk = new Int16Array([100, 200, 300, 400]);
    const ok = client.appendAudioChunk(chunk);
    expect(ok).toBe(true);

    const lastMsg = JSON.parse(activeMockWs!.sentMessages[activeMockWs!.sentMessages.length - 1]);
    expect(lastMsg.type).toBe('input_audio_buffer.append');
    expect(lastMsg.audio).toBe(pcm16ToBase64(chunk));

    client.disconnect();
  });

  test('Receives and decodes audio delta and live transcript delta', async () => {
    let receivedPcm: Int16Array | null = null;
    let receivedTranscript: string = '';
    let isFinalTranscript = false;

    let activeMockWs: MockWebSocket | null = null;
    const client = new OpenAiRealtimeClient(
      { targetLanguage: 'French' },
      {
        onAudioDelta: (chunk) => {
          receivedPcm = chunk;
        },
        onTranscriptDelta: (text, isFinal) => {
          receivedTranscript += text;
          isFinalTranscript = isFinal;
        }
      }
    );

    const MockWsFactory = class extends MockWebSocket {
      constructor(url: string, protocols: string[] = []) {
        super(url, protocols);
        activeMockWs = this;
      }
    };

    await client.connect(MockWsFactory as any);
    await new Promise((r) => setTimeout(r, 15));

    // Simulate server response.audio.delta
    const testAudio = new Int16Array([500, 1000, 1500]);
    activeMockWs!.mockReceiveMessage({
      type: 'response.audio.delta',
      delta: pcm16ToBase64(testAudio)
    });

    expect(receivedPcm).not.toBeNull();
    expect(receivedPcm!.length).toBe(3);
    expect(receivedPcm![0]).toBe(500);

    // Simulate server response.audio_transcript.delta
    activeMockWs!.mockReceiveMessage({
      type: 'response.audio_transcript.delta',
      delta: 'Bonjour '
    });
    activeMockWs!.mockReceiveMessage({
      type: 'response.audio_transcript.delta',
      delta: 'le monde'
    });

    expect(receivedTranscript).toBe('Bonjour le monde');
    expect(isFinalTranscript).toBe(false);

    client.disconnect();
  });

  test('Handles barge-in interruption when user starts speaking while response is active', async () => {
    let speechStartedCalled = false;
    let activeMockWs: MockWebSocket | null = null;

    const client = new OpenAiRealtimeClient(
      { targetLanguage: 'German' },
      {
        onInputSpeechStarted: () => {
          speechStartedCalled = true;
        }
      }
    );

    const MockWsFactory = class extends MockWebSocket {
      constructor(url: string, protocols: string[] = []) {
        super(url, protocols);
        activeMockWs = this;
      }
    };

    await client.connect(MockWsFactory as any);
    await new Promise((r) => setTimeout(r, 15));

    // Simulate active response started
    activeMockWs!.mockReceiveMessage({
      type: 'response.created',
      response: { id: 'resp_123' }
    });

    // Simulate user starts speaking
    activeMockWs!.mockReceiveMessage({
      type: 'input_audio_buffer.speech_started'
    });

    expect(speechStartedCalled).toBe(true);

    // Expect client to have sent input_audio_buffer.clear and response.cancel
    const messages = activeMockWs!.sentMessages.map((m) => JSON.parse(m));
    const cancelMsgs = messages.filter((m) => m.type === 'response.cancel');
    const clearMsgs = messages.filter((m) => m.type === 'input_audio_buffer.clear');

    expect(clearMsgs.length).toBeGreaterThan(0);
    expect(cancelMsgs.length).toBeGreaterThan(0);

    client.disconnect();
  });
});
