// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createClient } from "./client";
import { ClientProvider } from "./ClientProvider";
import { EVENT_GUARDS, RESULT_GUARDS } from "./guards";
import { createMockTransport } from "./mockTransport";
import { SessionProvider, useSession } from "./SessionProvider";
import type { Transport } from "./transport";
import type { BannerEvent, CaptionEvent } from "./types";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

// `import.meta.url` is not a file URL in the jsdom environment, so the path starts at the folder.
const FIXTURES = resolve(import.meta.dirname, "../../../../fixtures/ipc") + "/";
const fixture = (name: string): unknown => JSON.parse(readFileSync(FIXTURES + name, "utf8"));

const CAPTION = fixture("events/caption.json") as CaptionEvent;

/** A transport that answers every command with `result` and lets a test push events. */
function fakeTransport(result: unknown) {
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const transport: Transport = {
    invoke: () => Promise.resolve(result as never),
    listen(event, cb) {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => void set.delete(cb);
    },
  };
  const emit = (event: string, payload: unknown) =>
    listeners.get(event)?.forEach((cb) => cb(payload));
  return { transport, emit };
}

describe("the guards accept every wire fixture", () => {
  it.each([
    ["meeting://caption", "events/caption.json"],
    ["meeting://latency", "events/latency.json"],
    ["meeting://status", "events/status.json"],
    ["meeting://banner", "events/banner.json"],
    ["models://progress", "events/models_progress.json"],
  ] as const)("event %s", (event, file) => {
    expect(EVENT_GUARDS[event](fixture(file))).toBe(true);
  });

  it.each([
    ["check_setup", "setup_status.json"],
    ["list_models", "model_info_list.json"],
    ["start_meeting", "start_meeting_result.json"],
    ["stop_meeting", "start_meeting_result.json"],
    ["list_meetings", "meeting_summary_list.json"],
    ["get_settings", "app_settings.json"],
    ["list_notes", "note_list.json"],
    ["add_note", "note.json"],
  ] as const)("result of %s", (cmd, file) => {
    expect(RESULT_GUARDS[cmd](fixture(file))).toBe(true);
  });

  it("a command with no result accepts nothing, undefined and null only", () => {
    expect(RESULT_GUARDS.set_mute(null)).toBe(true);
    expect(RESULT_GUARDS.set_mute(undefined)).toBe(true);
    expect(RESULT_GUARDS.set_mute({})).toBe(false);
  });
});

describe("events from the shell are checked", () => {
  it("drops a caption event without `target` and never calls the listener", () => {
    const { transport, emit } = fakeTransport(undefined);
    const client = createClient(transport);
    const seen: CaptionEvent[] = [];
    client.onCaption((e) => seen.push(e));
    const withoutTarget: Partial<CaptionEvent> = { ...CAPTION };
    delete withoutTarget.target;
    expect(() => emit("meeting://caption", withoutTarget)).not.toThrow();
    expect(seen).toEqual([]);
    emit("meeting://caption", CAPTION);
    expect(seen).toEqual([CAPTION]);
  });

  it.each([
    ["meeting://caption", { ...CAPTION, direction: "sideways" }],
    ["meeting://caption", null],
    ["meeting://latency", { p50Ms: "820", p95Ms: 1 }],
    ["meeting://status", { state: "paused" }],
    ["meeting://banner", { kind: "fatal", message: "x" }],
    ["models://progress", { id: "tr", receivedBytes: 1, totalBytes: 2, state: "paused" }],
  ])("drops a malformed %s payload", (event, payload) => {
    const { transport, emit } = fakeTransport(undefined);
    const client = createClient(transport);
    const cb = vi.fn();
    client.onCaption(cb);
    client.onLatency(cb);
    client.onStatus(cb);
    client.onModelsProgress(cb);
    emit(event, payload);
    expect(cb).not.toHaveBeenCalled();
  });

  it("reports the first dropped event as one warning banner, not one per event", () => {
    const { transport, emit } = fakeTransport(undefined);
    const client = createClient(transport);
    const banners: BannerEvent[] = [];
    client.onBanner((b) => banners.push(b));
    client.onCaption(() => {});
    for (let n = 0; n < 5; n += 1) emit("meeting://caption", { lineId: `l${n}` });
    emit("meeting://status", { state: "nope" });
    expect(banners).toHaveLength(1);
    expect(banners[0].kind).toBe("warning");
    expect(banners[0].message).toMatch(/could not be read/);
  });

  it("a listener that unsubscribed gets no banner", () => {
    const { transport, emit } = fakeTransport(undefined);
    const client = createClient(transport);
    const cb = vi.fn();
    const off = client.onBanner(cb);
    client.onCaption(() => {});
    off();
    emit("meeting://caption", {});
    expect(cb).not.toHaveBeenCalled();
  });
});

describe("command results from the shell are checked", () => {
  it("turns a result that does not match the contract into a UI error", async () => {
    const { transport } = fakeTransport({ items: "not a list", canStart: true, missingGroups: [] });
    await expect(createClient(transport).checkSetup()).rejects.toMatchObject({
      code: "UNKNOWN",
      message: expect.stringMatching(/could not read/),
    });
  });

  it("rejects a list with one bad row, and a result of the wrong kind", async () => {
    const notes = [{ id: "n1", text: "ok", tag: null, createdAtMs: 1 }, { id: "n2" }];
    await expect(createClient(fakeTransport(notes).transport).listNotes()).rejects.toMatchObject({
      code: "UNKNOWN",
    });
    await expect(createClient(fakeTransport(undefined).transport).stopMeeting()).rejects.toEqual(
      expect.objectContaining({ code: "UNKNOWN" }),
    );
  });

  it("a command that returns nothing resolves with undefined for null", async () => {
    const { transport } = fakeTransport(null);
    await expect(createClient(transport).setMute({ direction: "out", muted: true })).resolves.toBe(
      undefined,
    );
  });

  it("keeps a rejection from the shell as it is", async () => {
    const transport: Transport = {
      invoke: () => Promise.reject({ code: "NO_MEETING", message: "No meeting." }),
      listen: () => () => {},
    };
    await expect(createClient(transport).stopMeeting()).rejects.toEqual({
      code: "NO_MEETING",
      message: "No meeting.",
    });
  });
});

describe("a malformed event does not reach the session", () => {
  let frames: FrameRequestCallback[] = [];
  beforeEach(() => {
    frames = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
    vi.stubGlobal("cancelAnimationFrame", () => {});
  });

  it("leaves the caption list empty and shows one warning", () => {
    const transport = createMockTransport({ autoEmit: false });
    function Probe() {
      const { session } = useSession();
      return (
        <p>
          lines:{session.captions.length} banner:{session.banner?.message ?? "none"}
        </p>
      );
    }
    render(
      <ClientProvider client={createClient(transport)}>
        <SessionProvider>
          <Probe />
        </SessionProvider>
      </ClientProvider>,
    );
    act(() => {
      for (let n = 0; n < 3; n += 1)
        transport.emit("meeting://caption", { lineId: `l${n}`, direction: "in", isFinal: true });
      const queued = frames;
      frames = [];
      queued.forEach((cb) => cb(0));
    });
    expect(screen.getByText(/lines:0/)).toBeTruthy();
    expect(screen.getByText(/could not be read/)).toBeTruthy();
  });
});
