/**
 * Types and interfaces for the Realtime Speech-to-Speech (S2S) translation engine.
 * Adheres to LiveMeetingCopilot clean-room specifications.
 */

export interface RealtimeClientConfig {
  apiKey?: string;
  endpointUrl?: string;
  model?: string;
  sourceLanguage?: string;
  targetLanguage: string;
  voice?: 'alloy' | 'echo' | 'shimmer' | 'ash' | 'ballad' | 'coral' | 'sage' | 'verse';
  instructions?: string;
  turnDetection?: {
    type: 'server_vad' | 'none';
    threshold?: number;
    prefix_padding_ms?: number;
    silence_duration_ms?: number;
  };
  sampleRate?: number; // Default 24000
}

export type RealtimeClientState = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'disconnected' | 'error';

/**
 * Minimal WebSocket surface used by the client. The browser/Bun `WebSocket`
 * satisfies it, and so does a lightweight test double.
 */
export interface RealtimeSocket {
  readyState: number;
  onopen: ((event: Event) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
  onerror: ((event: Event) => void) | null;
  onclose: ((event: CloseEvent) => void) | null;
  send(data: string): void;
  close(): void;
}

export type RealtimeSocketConstructor = new (url: string, protocols?: string[]) => RealtimeSocket;

export interface RealtimeServerEvent {
  type: string;
  event_id?: string;
  session?: Record<string, unknown>;
  delta?: string;
  item?: Record<string, unknown>;
  transcript?: string;
  response?: { id?: string; [key: string]: unknown };
  error?: {
    type: string;
    code: string;
    message: string;
  };
  [key: string]: unknown;
}

export interface RealtimeClientCallbacks {
  onStateChange?: (state: RealtimeClientState) => void;
  onAudioDelta?: (pcm16Chunk: Int16Array) => void;
  onTranscriptDelta?: (deltaText: string, isFinal: boolean) => void;
  onInputSpeechStarted?: () => void;
  onInputSpeechStopped?: () => void;
  onUserTranscript?: (text: string) => void;
  onError?: (error: Error) => void;
}
