import {
  RealtimeClientConfig,
  RealtimeClientState,
  RealtimeClientCallbacks,
  RealtimeServerEvent
} from './types';

/**
 * Base64 helper for Int16Array PCM audio
 */
export function pcm16ToBase64(pcmData: Int16Array): string {
  const uint8 = new Uint8Array(pcmData.buffer, pcmData.byteOffset, pcmData.byteLength);
  if (typeof Buffer !== 'undefined') {
    return Buffer.from(uint8).toString('base64');
  }
  let binary = '';
  const len = uint8.byteLength;
  for (let i = 0; i < len; i++) {
    binary += String.fromCharCode(uint8[i]);
  }
  return btoa(binary);
}

/**
 * Decode Base64 string to Int16Array PCM audio
 */
export function base64ToPcm16(base64: string): Int16Array {
  if (typeof Buffer !== 'undefined') {
    const buf = Buffer.from(base64, 'base64');
    return new Int16Array(buf.buffer, buf.byteOffset, buf.byteLength / 2);
  }
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return new Int16Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 2);
}

/**
 * Clean-room implementation of OpenAI Realtime Speech-to-Speech WebSocket Client.
 * Manages bidirectional audio streaming, live transcriptions, session parameters,
 * and barge-in interruption handling.
 */
export class OpenAiRealtimeClient {
  private ws: WebSocket | null = null;
  private state: RealtimeClientState = 'idle';
  private config: RealtimeClientConfig;
  private callbacks: RealtimeClientCallbacks;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private isExplicitDisconnect = false;
  private activeResponseId: string | null = null;

  constructor(config: RealtimeClientConfig, callbacks: RealtimeClientCallbacks = {}) {
    this.config = {
      model: 'gpt-4o-realtime-preview',
      endpointUrl: 'wss://api.openai.com/v1/realtime',
      voice: 'alloy',
      sampleRate: 24000,
      turnDetection: {
        type: 'server_vad',
        threshold: 0.5,
        prefix_padding_ms: 300,
        silence_duration_ms: 500
      },
      ...config
    };
    this.callbacks = callbacks;
  }

  public getState(): RealtimeClientState {
    return this.state;
  }

  public updateCallbacks(callbacks: Partial<RealtimeClientCallbacks>) {
    this.callbacks = { ...this.callbacks, ...callbacks };
  }

  private setState(newState: RealtimeClientState) {
    if (this.state !== newState) {
      this.state = newState;
      this.callbacks.onStateChange?.(newState);
    }
  }

  /**
   * Connect to Realtime WebSocket endpoint
   */
  public async connect(customWsConstructor?: any): Promise<void> {
    if (this.state === 'connected' || this.state === 'connecting') {
      return;
    }

    this.isExplicitDisconnect = false;
    this.setState('connecting');

    const url = new URL(this.config.endpointUrl || 'wss://api.openai.com/v1/realtime');
    if (this.config.model) {
      url.searchParams.set('model', this.config.model);
    }

    try {
      const WsClass = customWsConstructor || (typeof WebSocket !== 'undefined' ? WebSocket : null);
      if (!WsClass) {
        throw new Error('WebSocket implementation not available in current environment');
      }

      // Protocols / Headers for authentication
      const protocols = this.config.apiKey
        ? ['realtime', `openai-insecure-api-key.${this.config.apiKey}`, 'openai-beta.realtime-v1']
        : ['realtime', 'openai-beta.realtime-v1'];

      this.ws = new WsClass(url.toString(), protocols);

      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        this.setState('connected');
        this.sendSessionUpdate();
      };

      this.ws.onmessage = (event: MessageEvent) => {
        try {
          const rawData = typeof event.data === 'string' ? event.data : event.data.toString();
          const serverEvent: RealtimeServerEvent = JSON.parse(rawData);
          this.handleServerEvent(serverEvent);
        } catch (err: any) {
          this.callbacks.onError?.(new Error(`Failed to parse realtime event: ${err.message}`));
        }
      };

      this.ws.onerror = (event: any) => {
        const error = new Error(event?.message || 'WebSocket connection error');
        this.callbacks.onError?.(error);
      };

      this.ws.onclose = () => {
        if (!this.isExplicitDisconnect) {
          this.handleUnexpectedClose();
        } else {
          this.setState('disconnected');
        }
      };
    } catch (err: any) {
      this.setState('error');
      this.callbacks.onError?.(err);
      throw err;
    }
  }

  /**
   * Sends the session configuration update to prime the model for live meeting translation.
   */
  private sendSessionUpdate() {
    const src = this.config.sourceLanguage || 'Auto-detect';
    const tgt = this.config.targetLanguage;

    const defaultInstructions =
      `You are a high-speed, direct spoken translation engine for live meetings.\n` +
      `Your task is to translate any speech spoken in ${src} into natural, accurate ${tgt}.\n` +
      `CRITICAL RULES:\n` +
      `1. Directly speak the translation. Do NOT greet, do NOT explain, and do NOT add commentary.\n` +
      `2. Maintain the speaker's tone, professional nuances, and intent.\n` +
      `3. Never repeat, hallucinate, or echo background noise.`;

    const payload = {
      type: 'session.update',
      session: {
        modalities: ['audio', 'text'],
        instructions: this.config.instructions || defaultInstructions,
        voice: this.config.voice,
        input_audio_format: 'pcm16',
        output_audio_format: 'pcm16',
        input_audio_transcription: {
          model: 'whisper-1'
        },
        turn_detection: this.config.turnDetection
      }
    };

    this.send(payload);
  }

  /**
   * Streaming input audio append (Int16Array 24kHz Mono PCM)
   */
  public appendAudioChunk(pcm16: Int16Array): boolean {
    if (this.state !== 'connected' || !this.ws || this.ws.readyState !== 1) {
      return false;
    }

    const base64Audio = pcm16ToBase64(pcm16);
    this.send({
      type: 'input_audio_buffer.append',
      audio: base64Audio
    });
    return true;
  }

  /**
   * Manually commit audio buffer (used when turn detection is set to 'none' or manual push-to-talk)
   */
  public commitAudioBuffer(): void {
    if (this.state === 'connected') {
      this.send({ type: 'input_audio_buffer.commit' });
      this.send({ type: 'response.create' });
    }
  }

  /**
   * Clear buffer and cancel active response for barge-in / interruption
   */
  public interrupt(): void {
    if (this.state === 'connected') {
      this.send({ type: 'input_audio_buffer.clear' });
      if (this.activeResponseId) {
        this.send({ type: 'response.cancel' });
        this.activeResponseId = null;
      }
    }
  }

  /**
   * Dispatch incoming server events
   */
  private handleServerEvent(event: RealtimeServerEvent) {
    switch (event.type) {
      case 'session.created':
      case 'session.updated':
        // Session successfully established / updated
        break;

      case 'input_audio_buffer.speech_started':
        this.callbacks.onInputSpeechStarted?.();
        // Barge-in: if the model is currently speaking while user started talking, cancel model output
        if (this.activeResponseId) {
          this.interrupt();
        }
        break;

      case 'input_audio_buffer.speech_stopped':
        this.callbacks.onInputSpeechStopped?.();
        break;

      case 'conversation.item.input_audio_transcription.completed':
        if (event.transcript) {
          this.callbacks.onUserTranscript?.(event.transcript.trim());
        }
        break;

      case 'response.created':
        this.activeResponseId = event.response?.id || 'active';
        break;

      case 'response.audio.delta':
        if (event.delta) {
          const pcmChunk = base64ToPcm16(event.delta);
          this.callbacks.onAudioDelta?.(pcmChunk);
        }
        break;

      case 'response.audio_transcript.delta':
        if (event.delta) {
          this.callbacks.onTranscriptDelta?.(event.delta, false);
        }
        break;

      case 'response.audio_transcript.done':
        if (event.transcript) {
          this.callbacks.onTranscriptDelta?.(event.transcript, true);
        }
        break;

      case 'response.done':
        this.activeResponseId = null;
        break;

      case 'error':
        const err = new Error(event.error?.message || 'Realtime API error');
        this.callbacks.onError?.(err);
        break;

      default:
        break;
    }
  }

  private send(data: any): void {
    if (this.ws && this.ws.readyState === 1) {
      this.ws.send(JSON.stringify(data));
    }
  }

  private handleUnexpectedClose() {
    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;
      this.setState('reconnecting');
      const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 8000);
      setTimeout(() => {
        if (!this.isExplicitDisconnect) {
          this.connect();
        }
      }, delay);
    } else {
      this.setState('disconnected');
      this.callbacks.onError?.(new Error('Exceeded maximum reconnection attempts'));
    }
  }

  /**
   * Disconnect the client and release socket resources
   */
  public disconnect(): void {
    this.isExplicitDisconnect = true;
    if (this.ws) {
      try {
        this.ws.close();
      } catch (_) {}
      this.ws = null;
    }
    this.setState('disconnected');
  }
}
