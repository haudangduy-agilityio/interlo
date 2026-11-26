import type { Caption } from "../protocol";
import type { StartFailure } from "./startOutcome";
import type { BannerEvent, CaptionEvent, Direction, LatencyEvent, MeetingRunState } from "./types";

/** A line of the session: the caption and, when a caption event made it, that event's delay. */
export type SessionCaption = Caption & { delayMs?: number };

/** Everything the live screen, subtitle bar and compact panel show about the running meeting. */
export interface SessionState {
  status: MeetingRunState;
  meetingId: string | null;
  startedAtMs: number | null;
  captions: SessionCaption[];
  /** Ids of lines that left `captions` because of the cap, newest last. Their late updates are ignored. */
  evictedLineIds: readonly string[];
  latency: LatencyEvent | null;
  banner: BannerEvent | null;
  mutedOut: boolean;
  mutedIn: boolean;
  startFailure: StartFailure | null;
}

export const initialSession: SessionState = {
  status: "idle",
  meetingId: null,
  startedAtMs: null,
  captions: [],
  evictedLineIds: [],
  latency: null,
  banner: null,
  mutedOut: false,
  mutedIn: false,
  startFailure: null,
};

export type SessionAction =
  | { type: "startRequested" }
  | { type: "startFailed"; failure: StartFailure }
  | { type: "startFailureCleared" }
  | { type: "started"; meetingId: string; nowMs: number }
  | { type: "status"; state: MeetingRunState }
  | { type: "caption"; event: CaptionEvent; nowMs: number }
  /** Caption events that arrived within one animation frame, oldest first. */
  | { type: "captions"; items: readonly { event: CaptionEvent; nowMs: number }[] }
  | { type: "latency"; event: LatencyEvent }
  | { type: "banner"; event: BannerEvent }
  | { type: "dismissBanner" }
  | { type: "muted"; direction: Direction; muted: boolean };

/** Paused means both directions are muted while the meeting runs. */
export function isPaused(s: SessionState): boolean {
  return s.status === "running" && s.mutedOut && s.mutedIn;
}

/** Caption `sourceId` values: where the line came from. "in" events are the partner's speech. */
export const INCOMING_SOURCE_ID = "incoming";
export const OUTGOING_SOURCE_ID = "outgoing";

/** The partner's latest line (direction "in"), or null. The subtitle bar shows only this. */
export function latestIncomingCaption(captions: readonly Caption[]): Caption | null {
  for (let i = captions.length - 1; i >= 0; i -= 1) {
    if (captions[i].sourceId === INCOMING_SOURCE_ID) return captions[i];
  }
  return null;
}

export function captionFromEvent(e: CaptionEvent, startMs: number): SessionCaption {
  return {
    segmentId: e.lineId,
    sourceId: e.direction === "in" ? INCOMING_SOURCE_ID : OUTGOING_SOURCE_ID,
    startMs,
    sourceText: e.source,
    sourceStatus: e.isFinal ? "final" : "partial",
    targetText: e.target.length > 0 ? e.target : null,
    // The caption event does not carry the target language.
    targetLanguage: null,
    // A final line without a translation waits for none.
    translationStatus:
      e.target.length > 0 ? (e.isFinal ? "final" : "partial") : e.isFinal ? null : "pending",
    delayMs: e.delayMs,
  };
}

/**
 * The session keeps only this many caption lines, so a long meeting does not grow the list
 * or the work per update. The full transcript is the shell's store, not this array.
 */
export const MAX_CAPTION_LINES = 60;

/**
 * How many evicted line ids are remembered. Starting value: far more than the lines that can
 * still be updating at once (the shell sends one event per line per 50 ms window).
 */
export const MAX_EVICTED_LINE_IDS = 200;

function withCaption(state: SessionState, event: CaptionEvent, nowMs: number): SessionState {
  const caption = captionFromEvent(event, Math.max(0, nowMs - (state.startedAtMs ?? nowMs)));
  const index = state.captions.findIndex((c) => c.segmentId === caption.segmentId);
  if (index === -1) {
    // A late update for a line that the cap already removed must not come back as a new row.
    if (state.evictedLineIds.includes(caption.segmentId)) return state;
    const all = [...state.captions, caption];
    if (all.length <= MAX_CAPTION_LINES) return { ...state, captions: all };
    const cut = all.length - MAX_CAPTION_LINES;
    const evicted = [...state.evictedLineIds, ...all.slice(0, cut).map((c) => c.segmentId)];
    return {
      ...state,
      captions: all.slice(cut),
      evictedLineIds: evicted.slice(-MAX_EVICTED_LINE_IDS),
    };
  }
  // A later event for the same line keeps the line's first start time.
  const next = state.captions.slice();
  next[index] = { ...caption, startMs: state.captions[index].startMs };
  return { ...state, captions: next };
}

export function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case "startRequested":
      return { ...initialSession, status: "starting" };
    case "startFailed":
      return { ...initialSession, startFailure: action.failure };
    case "startFailureCleared":
      return state.startFailure === null ? state : { ...state, startFailure: null };
    case "started":
      return {
        ...state,
        meetingId: action.meetingId,
        startedAtMs: action.nowMs,
        startFailure: null,
      };
    case "status":
      if (action.state === "idle") {
        return {
          ...state,
          status: "idle",
          meetingId: null,
          startedAtMs: null,
          mutedOut: false,
          mutedIn: false,
        };
      }
      return { ...state, status: action.state };
    case "caption":
      return withCaption(state, action.event, action.nowMs);
    case "captions":
      return action.items.reduce((acc, i) => withCaption(acc, i.event, i.nowMs), state);
    case "latency":
      return { ...state, latency: action.event };
    case "banner":
      return { ...state, banner: action.event };
    case "dismissBanner":
      return { ...state, banner: null };
    case "muted":
      return action.direction === "out"
        ? { ...state, mutedOut: action.muted }
        : { ...state, mutedIn: action.muted };
  }
}
