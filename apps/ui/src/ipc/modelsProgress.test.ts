import { describe, expect, it } from "vitest";
import {
  downloadPercent,
  initialModelsProgress,
  modelsProgressReducer,
  type ModelsProgressState,
} from "./modelsProgress";
import type { ModelsProgressEvent } from "./types";

const event = (patch: Partial<ModelsProgressEvent> = {}): ModelsProgressEvent => ({
  id: "enV",
  receivedBytes: 10,
  totalBytes: 100,
  state: "downloading",
  ...patch,
});

const apply = (state: ModelsProgressState, e: ModelsProgressEvent) =>
  modelsProgressReducer(state, { type: "progress", event: e });

describe("models progress reducer", () => {
  it("keeps the latest event of each model id", () => {
    let s = apply(initialModelsProgress, event({ receivedBytes: 10 }));
    s = apply(s, event({ id: "tr", receivedBytes: 5 }));
    s = apply(s, event({ receivedBytes: 40 }));
    expect(s.byId.enV?.receivedBytes).toBe(40);
    expect(s.byId.tr?.receivedBytes).toBe(5);
  });

  it("ignores an id it does not know, as the contract says", () => {
    const s = apply(initialModelsProgress, event({ id: "mic" }));
    expect(s).toBe(initialModelsProgress);
    expect(apply(initialModelsProgress, event({ id: "" }))).toBe(initialModelsProgress);
  });

  it("ignores an event with a state it does not know", () => {
    const bad = event({ state: "paused" as unknown as ModelsProgressEvent["state"] });
    expect(apply(initialModelsProgress, bad)).toBe(initialModelsProgress);
  });

  it("counts a finished download so a screen can load the list again", () => {
    let s = apply(initialModelsProgress, event());
    expect(s.finished).toBe(0);
    s = apply(s, event({ state: "ok", receivedBytes: 100 }));
    expect(s.finished).toBe(1);
    expect(s.byId.enV?.state).toBe("ok");
    s = apply(s, event({ id: "tr", state: "ok", receivedBytes: 100 }));
    expect(s.finished).toBe(2);
  });

  it("keeps an error until the owner retries or dismisses it", () => {
    let s = apply(initialModelsProgress, event({ state: "error" }));
    expect(s.byId.enV?.state).toBe("error");
    s = apply(s, event({ id: "tr" }));
    expect(s.byId.enV?.state).toBe("error");
    s = apply(s, event({ state: "downloading", receivedBytes: 0 }));
    expect(s.byId.enV?.state).toBe("downloading");
  });

  it("clears one id and leaves the others", () => {
    let s = apply(initialModelsProgress, event());
    s = apply(s, event({ id: "tr" }));
    s = modelsProgressReducer(s, { type: "cleared", id: "enV" });
    expect(Object.keys(s.byId)).toEqual(["tr"]);
    expect(modelsProgressReducer(s, { type: "cleared", id: "viV" })).toBe(s);
  });
});

describe("downloadPercent", () => {
  it("is received over total as a whole percent, from 0 to 100", () => {
    expect(downloadPercent(event({ receivedBytes: 0, totalBytes: 200 }))).toBe(0);
    expect(downloadPercent(event({ receivedBytes: 50, totalBytes: 200 }))).toBe(25);
    expect(downloadPercent(event({ receivedBytes: 200, totalBytes: 200 }))).toBe(100);
  });

  it("is null when the total is not known, and never above 100", () => {
    expect(downloadPercent(event({ receivedBytes: 5, totalBytes: 0 }))).toBeNull();
    expect(downloadPercent(event({ receivedBytes: 500, totalBytes: 200 }))).toBe(100);
    expect(downloadPercent(event({ receivedBytes: -5, totalBytes: 200 }))).toBe(0);
  });
});
