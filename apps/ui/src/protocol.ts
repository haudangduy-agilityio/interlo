// Mirror of crates/meeting-core/src/protocol.rs (protocol v0.1).
// Contract test: src/protocol.test.ts reads the same fixture the Rust tests read.

export const SCHEMA_VERSION = "0.1";

export type Language = "en" | "vi";
export type SegmentStatus = "partial" | "stable" | "final";
export type TranslationStatus = "pending" | "partial" | "final" | "failed";
export type DataKind = "audio" | "text";
export type SessionStatus =
  "idle" | "preparing" | "running" | "paused" | "stopping" | "completed" | "failed";

export interface TranscriptSegment {
  segmentId: string;
  sourceId: string;
  revision: number;
  startMs: number;
  endMs: number;
  status: SegmentStatus;
  language: Language;
  text: string;
}

export interface TranslationSegment {
  translationId: string;
  sourceSegmentId: string;
  sourceRevision: number;
  targetLanguage: Language;
  text: string;
  status: TranslationStatus;
  providerId: string;
  revision: number;
}

export type EventPayload =
  | { eventType: "transcript.segment.updated"; payload: TranscriptSegment }
  | { eventType: "translation.segment.updated"; payload: TranslationSegment }
  | {
      eventType: "translation.segment.failed";
      payload: {
        translationId: string;
        sourceSegmentId: string;
        sourceRevision: number;
        providerId: string;
        reason: string;
      };
    }
  | { eventType: "session.status.changed"; payload: { from: SessionStatus; to: SessionStatus } }
  | { eventType: "speaker.label.updated"; payload: { speakerId: string; label: string } }
  | {
      eventType: "privacy.egress.blocked";
      payload: { providerId: string; dataKind: DataKind; reason: string };
    };

export interface Envelope {
  schemaVersion: string;
  eventId: string;
  sessionId: string;
  eventType: EventPayload["eventType"];
  sequence: number;
  occurredAt: string;
  payload: unknown;
}

export const KNOWN_EVENT_TYPES: ReadonlySet<string> = new Set<EventPayload["eventType"]>([
  "transcript.segment.updated",
  "translation.segment.updated",
  "translation.segment.failed",
  "session.status.changed",
  "speaker.label.updated",
  "privacy.egress.blocked",
]);

/** One caption row, as the Rust coordinator reports it. */
export interface Caption {
  segmentId: string;
  sourceId: string;
  startMs: number;
  sourceText: string;
  sourceStatus: SegmentStatus;
  targetText: string | null;
  targetLanguage: Language | null;
  translationStatus: TranslationStatus | null;
}

/** Parse one JSON line into an envelope. Throws on malformed input. */
export function parseEnvelope(line: string): Envelope {
  const obj = JSON.parse(line) as Partial<Envelope>;
  if (obj.schemaVersion !== SCHEMA_VERSION) {
    throw new Error(`unsupported schemaVersion ${String(obj.schemaVersion)}`);
  }
  if (typeof obj.eventId !== "string" || typeof obj.sessionId !== "string") {
    throw new Error("missing eventId or sessionId");
  }
  if (typeof obj.sequence !== "number" || !Number.isInteger(obj.sequence) || obj.sequence < 1) {
    throw new Error("sequence must be a positive integer");
  }
  if (typeof obj.eventType !== "string" || !KNOWN_EVENT_TYPES.has(obj.eventType)) {
    throw new Error(`unknown eventType ${String(obj.eventType)}`);
  }
  return obj as Envelope;
}
