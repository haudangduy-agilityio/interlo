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

export interface RealtimeServerEvent {
  type: string;
  event_id?: string;
  session?: any;
  delta?: string;
  item?: any;
  transcript?: string;
  response?: any;
  error?: {
    type: string;
    code: string;
    message: string;
  };
  [key: string]: any;
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
