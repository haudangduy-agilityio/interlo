import { describe, expect, it } from "vitest";
import {
  MAX_CAPTION_LINES,
  MAX_EVICTED_LINE_IDS,
  captionFromEvent,
  initialSession,
  isPaused,
  latestIncomingCaption,
  sessionReducer,
  type SessionState,
} from "./meetingSession";
import type { CaptionEvent } from "./types";

const running: SessionState = {
  ...initialSession,
  status: "running",
  meetingId: "m1",
  startedAtMs: 1000,
};

const partial: CaptionEvent = {
  lineId: "l1",
  direction: "in",
  isFinal: false,
  source: "We can",
  target: "",
  delayMs: 300,
};

describe("sessionReducer", () => {
  it("maps a caption event to a protocol caption and updates it by line id", () => {
    const first = sessionReducer(running, { type: "caption", event: partial, nowMs: 3500 });
    expect(first.captions).toEqual([
      {
        segmentId: "l1",
        sourceId: "incoming",
        startMs: 2500,
        sourceText: "We can",
        sourceStatus: "partial",
        targetText: null,
        targetLanguage: null,
        translationStatus: "pending",
        delayMs: 300,
      },
    ]);
    const second = sessionReducer(first, {
      type: "caption",
      event: { ...partial, isFinal: true, source: "We can ship.", target: "Chúng ta có thể giao." },
      nowMs: 9000,
    });
    expect(second.captions).toHaveLength(1);
    expect(second.captions[0]).toMatchObject({
      sourceStatus: "final",
      targetText: "Chúng ta có thể giao.",
      startMs: 2500,
    });
  });

  it("a final line without a translation waits for none", () => {
    const caption = captionFromEvent({ ...partial, isFinal: true, target: "" }, 0);
    expect(caption.targetText).toBeNull();
    expect(caption.translationStatus).toBeNull();
  });

  it("is paused only when running and both directions are muted", () => {
    const outOnly = sessionReducer(running, { type: "muted", direction: "out", muted: true });
    expect(isPaused(outOnly)).toBe(false);
    const both = sessionReducer(outOnly, { type: "muted", direction: "in", muted: true });
    expect(isPaused(both)).toBe(true);
    expect(isPaused({ ...both, status: "idle" })).toBe(false);
  });

  it("a failed start returns to idle and keeps the failure for the screen", () => {
    const starting = sessionReducer(initialSession, { type: "startRequested" });
    expect(starting.status).toBe("starting");
    const failed = sessionReducer(starting, {
      type: "startFailed",
      failure: { kind: "setup-incomplete", message: "x" },
    });
    expect(failed.status).toBe("idle");
    expect(failed.startFailure).toEqual({ kind: "setup-incomplete", message: "x" });
  });

  it("idle clears mute flags but keeps captions for review", () => {
    const withCaption = sessionReducer(
      { ...running, mutedOut: true },
      { type: "caption", event: partial, nowMs: 2000 },
    );
    const idle = sessionReducer(withCaption, { type: "status", state: "idle" });
    expect(idle).toMatchObject({ status: "idle", meetingId: null, mutedOut: false });
    expect(idle.captions).toHaveLength(1);
  });
});

describe("sessionReducer caption cap", () => {
  const event = (n: number): CaptionEvent => ({
    lineId: `l${n}`,
    direction: "in",
    isFinal: true,
    source: `Line ${n}`,
    target: "",
    delayMs: 300,
  });

  it("keeps the latest 60 lines after 100 events", () => {
    let state = running;
    for (let n = 1; n <= 100; n += 1) {
      state = sessionReducer(state, { type: "caption", event: event(n), nowMs: 2000 + n });
    }
    expect(state.captions).toHaveLength(60);
    expect(state.captions[0].segmentId).toBe("l41");
    expect(state.captions[59].segmentId).toBe("l100");
  });

  it("still updates a line in place at the cap, without dropping another line", () => {
    let state = running;
    for (let n = 1; n <= 60; n += 1) {
      state = sessionReducer(state, { type: "caption", event: event(n), nowMs: 2000 });
    }
    state = sessionReducer(state, {
      type: "caption",
      event: { ...event(30), source: "Line 30 corrected" },
      nowMs: 3000,
    });
    expect(state.captions).toHaveLength(60);
    expect(state.captions[0].segmentId).toBe("l1");
    expect(state.captions[29].sourceText).toBe("Line 30 corrected");
  });

  it("applies a batch of caption events in order, like single events", () => {
    const batch = sessionReducer(running, {
      type: "captions",
      items: [1, 2, 3].map((n) => ({ event: event(n), nowMs: 2000 + n })),
    });
    expect(batch.captions.map((c) => c.segmentId)).toEqual(["l1", "l2", "l3"]);
  });
});

describe("sessionReducer late update for an evicted line", () => {
  const event = (n: number, source = `Line ${n}`): CaptionEvent => ({
    lineId: `l${n}`,
    direction: "in",
    isFinal: false,
    source,
    target: "",
    delayMs: 300,
  });
  const feed = (state: SessionState, from: number, to: number): SessionState => {
    let next = state;
    for (let n = from; n <= to; n += 1)
      next = sessionReducer(next, { type: "caption", event: event(n), nowMs: 2000 });
    return next;
  };

  it("ignores an update for the line the cap just removed (cap + 1 lines)", () => {
    const full = feed(running, 1, MAX_CAPTION_LINES + 1);
    expect(full.captions).toHaveLength(MAX_CAPTION_LINES);
    expect(full.captions[0].segmentId).toBe("l2");
    const late = sessionReducer(full, {
      type: "caption",
      event: event(1, "Line 1 late"),
      nowMs: 3000,
    });
    expect(late.captions).toHaveLength(MAX_CAPTION_LINES);
    expect(late.captions.map((c) => c.segmentId)).not.toContain("l1");
    expect(late.captions[MAX_CAPTION_LINES - 1].segmentId).toBe(`l${MAX_CAPTION_LINES + 1}`);
  });

  it("still updates the oldest kept line, which is not evicted yet", () => {
    const full = feed(running, 1, MAX_CAPTION_LINES + 1);
    const updated = sessionReducer(full, {
      type: "caption",
      event: event(2, "Line 2 corrected"),
      nowMs: 3000,
    });
    expect(updated.captions[0]).toMatchObject({ segmentId: "l2", sourceText: "Line 2 corrected" });
    expect(updated.captions).toHaveLength(MAX_CAPTION_LINES);
  });

  it("ignores a batch update for an evicted line too", () => {
    const full = feed(running, 1, MAX_CAPTION_LINES + 5);
    const next = sessionReducer(full, {
      type: "captions",
      items: [{ event: event(3, "late"), nowMs: 3000 }],
    });
    expect(next.captions).toEqual(full.captions);
  });

  it("remembers at most MAX_EVICTED_LINE_IDS evicted ids, oldest first out", () => {
    const total = MAX_CAPTION_LINES + MAX_EVICTED_LINE_IDS + 10;
    const state = feed(running, 1, total);
    expect(state.evictedLineIds).toHaveLength(MAX_EVICTED_LINE_IDS);
    expect(state.evictedLineIds[MAX_EVICTED_LINE_IDS - 1]).toBe(`l${total - MAX_CAPTION_LINES}`);
    expect(state.evictedLineIds).not.toContain("l1");
  });

  it("a new meeting forgets the evicted ids", () => {
    const full = feed(running, 1, MAX_CAPTION_LINES + 1);
    const fresh = sessionReducer(full, { type: "startRequested" });
    expect(fresh.evictedLineIds).toEqual([]);
  });
});

describe("latestIncomingCaption", () => {
  it("returns the newest incoming line, or null when there is none", () => {
    const mk = (id: string, dir: "in" | "out"): CaptionEvent => ({
      lineId: id,
      direction: dir,
      isFinal: true,
      source: id,
      target: "",
      delayMs: 300,
    });
    let state = running;
    expect(latestIncomingCaption(state.captions)).toBeNull();
    state = sessionReducer(state, { type: "caption", event: mk("a", "in"), nowMs: 1100 });
    state = sessionReducer(state, { type: "caption", event: mk("b", "out"), nowMs: 1200 });
    expect(latestIncomingCaption(state.captions)?.segmentId).toBe("a");
  });
});
