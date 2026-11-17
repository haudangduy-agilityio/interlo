import { beforeEach, describe, expect, test } from 'bun:test';
import {
  OpenAiRealtimeClient,
  pcm16ToBase64,
  base64ToPcm16
} from '../OpenAiRealtimeClient';
import { RealtimeClientState, RealtimeSocket } from '../types';

/**
 * Shape of the JSON messages the client sends over the socket.
 */
interface SentMessage {
  type: string;
  audio?: string;
  session?: { voice?: string; instructions?: string };
}

/**
 * Mock WebSocket implementation for unit testing
 */
class MockWebSocket implements RealtimeSocket {
  public static instances: MockWebSocket[] = [];

  public static latest(): MockWebSocket {
    const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    if (!ws) {
      throw new Error('No MockWebSocket was created');
    }
    return ws;
  }

  public url: string;
  public protocols: string[];
  public readyState: number = 0; // 0: CONNECTING, 1: OPEN, 2: CLOSING, 3: CLOSED
  public sentMessages: string[] = [];

  public onopen: ((event: Event) => void) | null = null;
  public onmessage: ((event: MessageEvent) => void) | null = null;
  public onerror: ((event: Event) => void) | null = null;
  public onclose: ((event: CloseEvent) => void) | null = null;

  constructor(url: string, protocols: string[] = []) {
    this.url = url;
    this.protocols = protocols;
    MockWebSocket.instances.push(this);
    setTimeout(() => {
      this.readyState = 1;
      this.onopen?.(new Event('open'));
    }, 5);
  }

  public send(data: string) {
    this.sentMessages.push(data);
  }

  public close() {
    this.readyState = 3;
    this.onclose?.(new CloseEvent('close'));
  }

  // Helper to simulate incoming server message
  public mockReceiveMessage(obj: Record<string, unknown>) {
    this.onmessage?.(new MessageEvent('message', { data: JSON.stringify(obj) }));
  }

  public parsedMessages(): SentMessage[] {
    return this.sentMessages.map((m) => JSON.parse(m) as SentMessage);
  }
}

describe('OpenAiRealtimeClient', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
  });

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

    await client.connect(MockWebSocket);

    // Wait for mock connection open
    await new Promise((r) => setTimeout(r, 20));

    expect(client.getState()).toBe('connected');
    expect(states).toContain('connecting');
    expect(states).toContain('connected');

    const sent = MockWebSocket.latest().parsedMessages();
    expect(sent.length).toBeGreaterThan(0);

    const firstMsg = sent[0];
    expect(firstMsg?.type).toBe('session.update');
    expect(firstMsg?.session?.voice).toBe('shimmer');
    expect(firstMsg?.session?.instructions).toContain('English');
    expect(firstMsg?.session?.instructions).toContain('Vietnamese');

    client.disconnect();
    expect(client.getState()).toBe('disconnected');
  });

  test('Appends PCM audio buffer chunk formatted as base64', async () => {
    const client = new OpenAiRealtimeClient(
      { targetLanguage: 'Spanish' },
      {}
    );

    await client.connect(MockWebSocket);
    await new Promise((r) => setTimeout(r, 15));

    const chunk = new Int16Array([100, 200, 300, 400]);
    const ok = client.appendAudioChunk(chunk);
    expect(ok).toBe(true);

    const sent = MockWebSocket.latest().parsedMessages();
    const lastMsg = sent[sent.length - 1];
    expect(lastMsg?.type).toBe('input_audio_buffer.append');
    expect(lastMsg?.audio).toBe(pcm16ToBase64(chunk));

    client.disconnect();
  });

  test('Receives and decodes audio delta and live transcript delta', async () => {
    let receivedPcm: Int16Array | null = null;
    let receivedTranscript: string = '';
    let isFinalTranscript = false;

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

    await client.connect(MockWebSocket);
    await new Promise((r) => setTimeout(r, 15));

    // Simulate server response.audio.delta
    const testAudio = new Int16Array([500, 1000, 1500]);
    MockWebSocket.latest().mockReceiveMessage({
      type: 'response.audio.delta',
      delta: pcm16ToBase64(testAudio)
    });

    expect(receivedPcm).not.toBeNull();
    expect(receivedPcm!.length).toBe(3);
    expect(receivedPcm![0]).toBe(500);

    // Simulate server response.audio_transcript.delta
    MockWebSocket.latest().mockReceiveMessage({
      type: 'response.audio_transcript.delta',
      delta: 'Bonjour '
    });
    MockWebSocket.latest().mockReceiveMessage({
      type: 'response.audio_transcript.delta',
      delta: 'le monde'
    });

    expect(receivedTranscript).toBe('Bonjour le monde');
    expect(isFinalTranscript).toBe(false);

    client.disconnect();
  });

  test('Handles barge-in interruption when user starts speaking while response is active', async () => {
    let speechStartedCalled = false;

    const client = new OpenAiRealtimeClient(
      { targetLanguage: 'German' },
      {
        onInputSpeechStarted: () => {
          speechStartedCalled = true;
        }
      }
    );

    await client.connect(MockWebSocket);
    await new Promise((r) => setTimeout(r, 15));

    // Simulate active response started
    MockWebSocket.latest().mockReceiveMessage({
      type: 'response.created',
      response: { id: 'resp_123' }
    });

    // Simulate user starts speaking
    MockWebSocket.latest().mockReceiveMessage({
      type: 'input_audio_buffer.speech_started'
    });

    expect(speechStartedCalled).toBe(true);

    // Expect client to have sent input_audio_buffer.clear and response.cancel
    const messages = MockWebSocket.latest().parsedMessages();
    const cancelMsgs = messages.filter((m) => m.type === 'response.cancel');
    const clearMsgs = messages.filter((m) => m.type === 'input_audio_buffer.clear');

    expect(clearMsgs.length).toBeGreaterThan(0);
    expect(cancelMsgs.length).toBeGreaterThan(0);

    client.disconnect();
  });
});
