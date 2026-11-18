// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createClient } from "./client";
import { ClientProvider } from "./ClientProvider";
import { createMockTransport } from "./mockTransport";
import { SessionProvider, useSession } from "./SessionProvider";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

let frames: FrameRequestCallback[] = [];
beforeEach(() => {
  frames = [];
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
  vi.stubGlobal("cancelAnimationFrame", () => {});
});
const runFrame = () =>
  act(() => {
    const queued = frames;
    frames = [];
    for (const cb of queued) cb(0);
  });

describe("SessionProvider caption events", () => {
  function setup() {
    const transport = createMockTransport({ autoEmit: false });
    let renders = 0;
    function Probe() {
      const { session } = useSession();
      renders += 1;
      return (
        <p>
          lines:{session.captions.length} status:{session.status}
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
    const caption = (n: number) =>
      act(() =>
        transport.emit("meeting://caption", {
          lineId: `l${n}`,
          direction: "in",
          isFinal: true,
          source: `Line ${n}`,
          target: "",
        }),
      );
    return { transport, caption, renders: () => renders };
  }

  it("flushes many caption events once per animation frame", () => {
    const { caption, renders } = setup();
    const before = renders();
    for (let n = 1; n <= 50; n += 1) caption(n);
    // Nothing is committed until the frame runs, and only one frame is asked for.
    expect(renders()).toBe(before);
    expect(frames).toHaveLength(1);

    runFrame();
    expect(screen.getByText(/lines:50/)).toBeTruthy();
    expect(renders()).toBe(before + 1);
  });

  it("does not hold events without limit when frames do not run (a hidden window)", () => {
    const { caption } = setup();
    for (let n = 1; n <= 450; n += 1) caption(n);
    // No frame ran, but the buffer was handed to the session before it grew past its limit.
    expect(screen.getByText(/lines:60/)).toBeTruthy();
  });

  it("keeps order with a status event that arrives between captions", () => {
    const { transport, caption } = setup();
    caption(1);
    act(() => transport.emit("meeting://status", { state: "idle" }));
    // The status change flushes the buffered caption first, so no line arrives after "idle".
    expect(screen.getByText(/lines:1 status:idle/)).toBeTruthy();
    runFrame();
    expect(screen.getByText(/lines:1 status:idle/)).toBeTruthy();
  });
});

describe("SessionProvider model progress events", () => {
  function setup() {
    const transport = createMockTransport({ autoEmit: false });
    function Probe() {
      const { models, clearModelProgress } = useSession();
      return (
        <div>
          <p>enV:{models.byId.enV?.state ?? "none"}</p>
          <p>received:{models.byId.enV?.receivedBytes ?? "-"}</p>
          <p>finished:{models.finished}</p>
          <button onClick={() => clearModelProgress("enV")}>clear</button>
        </div>
      );
    }
    render(
      <ClientProvider client={createClient(transport)}>
        <SessionProvider>
          <Probe />
        </SessionProvider>
      </ClientProvider>,
    );
    const progress = (patch: Record<string, unknown>) =>
      act(() =>
        transport.emit("models://progress", {
          id: "enV",
          receivedBytes: 10,
          totalBytes: 100,
          state: "downloading",
          ...patch,
        }),
      );
    return { progress };
  }

  it("routes models://progress events to the models state", () => {
    const { progress } = setup();
    expect(screen.getByText("enV:none")).toBeTruthy();
    progress({ receivedBytes: 30 });
    expect(screen.getByText("enV:downloading")).toBeTruthy();
    expect(screen.getByText("received:30")).toBeTruthy();
    progress({ state: "ok", receivedBytes: 100 });
    expect(screen.getByText("enV:ok")).toBeTruthy();
    expect(screen.getByText("finished:1")).toBeTruthy();
  });

  it("ignores an event for an id it does not know", () => {
    const { progress } = setup();
    progress({ id: "mic" });
    expect(screen.getByText("enV:none")).toBeTruthy();
  });

  it("clearModelProgress forgets one model", () => {
    const { progress } = setup();
    progress({ state: "error" });
    expect(screen.getByText("enV:error")).toBeTruthy();
    act(() => screen.getByRole("button", { name: "clear" }).click());
    expect(screen.getByText("enV:none")).toBeTruthy();
  });
});
