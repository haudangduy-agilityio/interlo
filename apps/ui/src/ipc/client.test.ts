import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { invoke as tauriCoreInvoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { createClient, createDefaultClient, selectTransport } from "./client";
import { createMockTransport, sampleModels } from "./mockTransport";
import { requestStart } from "./startOutcome";
import { createTauriTransport } from "./tauriTransport";
import { startArgs } from "../shared/testing/startArgs";
import type { CaptionEvent, ModelsProgressEvent } from "./types";

// The real package needs the Tauri shell; the test replaces its one call.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function mockClient(options = {}) {
  const transport = createMockTransport({ autoEmit: false, ...options });
  return { transport, client: createClient(transport) };
}

const SETUP_MESSAGE = "Finish setup first. Not turned on yet: Models.";

describe("mock client returns the contract shapes", () => {
  it("check_setup: everything ok except the English voice model", async () => {
    const { client } = mockClient();
    const status = await client.checkSetup();
    expect(status.items.map((i) => i.id).sort()).toEqual([
      "capture",
      "enV",
      "mic",
      "phones",
      "sr",
      "tr",
      "viV",
      "vmic",
    ]);
    expect(status.items.filter((i) => i.state !== "ok").map((i) => i.id)).toEqual(["enV"]);
    expect(status.canStart).toBe(false);
    expect(status.missingGroups).toEqual(["Models"]);
    for (const item of status.items) {
      expect(["devices", "virtualMicrophone", "models"]).toContain(item.group);
      expect(typeof item.detail).toBe("string");
    }
    const groupOf = (id: string) => status.items.find((i) => i.id === id)?.group;
    expect(["mic", "phones", "capture"].map(groupOf)).toEqual(["devices", "devices", "devices"]);
    expect(groupOf("vmic")).toBe("virtualMicrophone");
    expect(["sr", "tr", "enV", "viV"].map(groupOf)).toEqual([
      "models",
      "models",
      "models",
      "models",
    ]);
  });

  it("list_models ids equal the setup item ids of the models", async () => {
    const { client } = mockClient();
    const modelIds = (await client.listModels()).map((m) => m.id);
    expect(modelIds).toEqual(["sr", "tr", "enV", "viV"]);
    const setupIds = (await client.checkSetup()).items
      .filter((i) => i.group === "models")
      .map((i) => i.id);
    expect(setupIds).toEqual(modelIds);
  });

  it("list_models, list_meetings, get_settings and list_notes", async () => {
    const { client } = mockClient();
    const models = await client.listModels();
    expect(models[0]).toEqual({
      id: expect.any(String),
      name: expect.any(String),
      sizeBytes: expect.any(Number),
      state: expect.any(String),
      // UI-side only (not in docs/ipc-contract.md): the licence the model manager shows.
      licence: expect.any(String),
      nonCommercial: expect.any(Boolean),
    });
    expect(models.filter((m) => m.nonCommercial).map((m) => m.id)).toEqual(["tr"]);
    const meetings = await client.listMeetings();
    expect(meetings.map((m) => m.status).sort()).toEqual([
      "failed",
      "generating",
      "interrupted",
      "ready",
    ]);
    expect(meetings[0]).toEqual({
      id: expect.any(String),
      title: expect.any(String),
      startedAtMs: expect.any(Number),
      durationSec: expect.any(Number),
      status: expect.any(String),
    });
    expect(await client.getSettings()).toEqual({
      uiLanguage: "en",
      ducking: true,
      autoDeleteDays: null,
    });
    const notes = await client.listNotes();
    expect(notes[0]).toEqual({
      id: expect.any(String),
      text: expect.any(String),
      tag: expect.anything(),
      createdAtMs: expect.any(Number),
    });
  });

  it("add_note returns the new note and list_notes shows it first", async () => {
    const { client } = mockClient();
    const note = await client.addNote({ text: "  Call back  ", tag: "  " });
    expect(note).toMatchObject({ text: "Call back", tag: null });
    expect((await client.listNotes())[0].id).toBe(note.id);
  });

  it("set_setting stores a valid value and rejects an invalid one", async () => {
    const { client } = mockClient();
    await client.setSetting({ key: "uiLanguage", value: "vi" });
    expect((await client.getSettings()).uiLanguage).toBe("vi");
    await expect(
      client.setSetting({ key: "ducking", value: "yes" as unknown as boolean }),
    ).rejects.toMatchObject({ code: "INVALID_INPUT" });
  });
});

describe("mock client errors", () => {
  it("start_meeting rejects with SETUP_INCOMPLETE and names only the Models group", async () => {
    const { client } = mockClient();
    await expect(client.startMeeting(startArgs())).rejects.toEqual({
      code: "SETUP_INCOMPLETE",
      message: SETUP_MESSAGE,
    });
  });

  it("requestStart keeps the command's message word for word", async () => {
    const { client } = mockClient();
    const outcome = await requestStart(client, startArgs());
    expect(outcome).toEqual({ kind: "setup-incomplete", message: SETUP_MESSAGE });
    expect(SETUP_MESSAGE).not.toContain("Virtual microphone");
  });

  it("stop_meeting and set_mute reject with NO_MEETING when nothing runs", async () => {
    const { client } = mockClient();
    await expect(client.stopMeeting()).rejects.toMatchObject({ code: "NO_MEETING" });
    await expect(client.setMute({ direction: "out", muted: true })).rejects.toMatchObject({
      code: "NO_MEETING",
    });
  });

  it("an empty note is INVALID_INPUT and an unknown command is NOT_IMPLEMENTED", async () => {
    const { client, transport } = mockClient();
    await expect(client.addNote({ text: "   ", tag: null })).rejects.toMatchObject({
      code: "INVALID_INPUT",
    });
    await expect(transport.invoke("export_everything")).rejects.toMatchObject({
      code: "NOT_IMPLEMENTED",
    });
  });
});

describe("mock client after setup is finished", () => {
  it("download_model fixes setup, then start and stop work and a second start is refused", async () => {
    const { client } = mockClient();
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    expect((await client.checkSetup()).canStart).toBe(true);
    const { meetingId } = await client.startMeeting(startArgs({ mode: "listen" }));
    expect(meetingId).toMatch(/^mtg_/);
    await expect(client.startMeeting(startArgs({ mode: "listen" }))).rejects.toMatchObject({
      code: "MEETING_RUNNING",
    });
    await client.setMute({ direction: "in", muted: true });
    expect(await client.stopMeeting()).toEqual({ meetingId });
  });
});

describe("events", () => {
  it("delivers a caption event to a listener and stops after unsubscribe", () => {
    const { client, transport } = mockClient();
    const seen: CaptionEvent[] = [];
    const off = client.onCaption((e) => seen.push(e));
    const event: CaptionEvent = {
      lineId: "l1",
      direction: "in",
      isFinal: true,
      source: "Hello",
      target: "Xin chào",
      delayMs: 450,
    };
    transport.emit("meeting://caption", event);
    off();
    transport.emit("meeting://caption", event);
    expect(seen).toEqual([event]);
  });
});

describe("transport choice", () => {
  it("falls back to the mock in development when there is no Tauri global", async () => {
    expect((await selectTransport()).kind).toBe("mock");
  });

  it("refuses to load the mock in a production build without the Tauri global", async () => {
    vi.stubEnv("DEV", false);
    try {
      await expect(createDefaultClient()).rejects.toThrow("The desktop shell is not connected.");
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it("uses the Tauri transport inside the shell (no global API needed), and forwards invoke", async () => {
    const invoke = vi.mocked(tauriCoreInvoke);
    invoke.mockResolvedValue({ meetingId: "m1" });
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    try {
      const picked = await selectTransport();
      expect(picked.kind).toBe("tauri");
      await expect(createClient(picked.transport, picked.kind).stopMeeting()).resolves.toEqual({
        meetingId: "m1",
      });
      expect(invoke).toHaveBeenCalledWith("stop_meeting", undefined);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("turns a string rejection from the shell into an error object", async () => {
    const invoke = vi.fn().mockRejectedValue("boom");
    const transport = createTauriTransport({ invoke, listen: vi.fn() });
    await expect(createClient(transport, "tauri").listNotes()).rejects.toEqual({
      code: "UNKNOWN",
      message: "boom",
    });
  });

  it("unsubscribes from a Tauri listener even when it registers late", async () => {
    const unlisten = vi.fn();
    const listen = vi.fn().mockResolvedValue(unlisten);
    const transport = createTauriTransport({ invoke: vi.fn(), listen });
    const off = transport.listen("meeting://status", () => {});
    off();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});

describe("mock client input rules", () => {
  it("start_meeting rejects an unknown mode with INVALID_INPUT before the setup check", async () => {
    const { client } = mockClient();
    await expect(
      client.startMeeting(startArgs({ mode: "shout" as unknown as "both" })),
    ).rejects.toMatchObject({ code: "INVALID_INPUT" });
  });

  it("start_meeting checks the mode before MEETING_RUNNING", async () => {
    const { client } = mockClient();
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    await client.startMeeting(startArgs());
    await expect(
      client.startMeeting(startArgs({ mode: "Both" as unknown as "both" })),
    ).rejects.toMatchObject({
      code: "INVALID_INPUT",
    });
    await expect(client.startMeeting(startArgs())).rejects.toMatchObject({
      code: "MEETING_RUNNING",
    });
  });

  it("set_setting rejects an unknown key with INVALID_INPUT", async () => {
    const { client } = mockClient();
    await expect(
      client.setSetting({ key: "theme" as unknown as "ducking", value: true }),
    ).rejects.toMatchObject({ code: "INVALID_INPUT" });
  });

  it("download_model rejects an unknown id with INVALID_INPUT, even when the licence is accepted", async () => {
    const { client } = mockClient();
    for (const id of ["nope", "SR", "", "mic"]) {
      await expect(client.downloadModel({ id, licenceAccepted: true })).rejects.toEqual({
        code: "INVALID_INPUT",
        message: `Unknown model: ${id}.`,
      });
    }
  });

  it("download_model rejects licenceAccepted false with INVALID_INPUT and downloads nothing", async () => {
    const { client } = mockClient();
    await expect(client.downloadModel({ id: "enV", licenceAccepted: false })).rejects.toEqual({
      code: "INVALID_INPUT",
      message: "Accept the licence first.",
    });
    const enV = (await client.listModels()).find((m) => m.id === "enV");
    expect(enV?.state).toBe("missing");
  });

  it("download_model refuses an unknown id before it looks at the licence", async () => {
    const { client } = mockClient();
    await expect(client.downloadModel({ id: "nope", licenceAccepted: false })).rejects.toEqual({
      code: "INVALID_INPUT",
      message: "Unknown model: nope.",
    });
  });

  it("list_models returns a licence line and the non-commercial flag for every model", async () => {
    const { client } = mockClient();
    const models = await client.listModels();
    for (const m of models) {
      expect(typeof m.licence).toBe("string");
      expect(m.licence.length).toBeGreaterThan(0);
      expect(typeof m.nonCommercial).toBe("boolean");
    }
    expect(models.find((m) => m.id === "tr")?.nonCommercial).toBe(true);
  });

  it("cancel_model_download rejects an unknown id with INVALID_INPUT and resolves for a known id", async () => {
    const { client } = mockClient();
    for (const id of ["nope", "SR", "", "mic"]) {
      await expect(client.cancelModelDownload({ id })).rejects.toEqual({
        code: "INVALID_INPUT",
        message: `Unknown model: ${id}.`,
      });
    }
    await expect(client.cancelModelDownload({ id: "enV" })).resolves.toBeUndefined();
  });

  it("set_mute rejects NO_MEETING without a meeting, and a bad shape like the bridge does", async () => {
    const { client } = mockClient();
    await expect(client.setMute({ direction: "out", muted: true })).rejects.toMatchObject({
      code: "NO_MEETING",
    });
    // The shell's bridge refuses a value that is not "out" or "in" with a plain string, also
    // when no meeting runs. The UI turns that into code UNKNOWN.
    await expect(
      client.setMute({ direction: "sideways" as unknown as "out", muted: true }),
    ).rejects.toMatchObject({ code: "UNKNOWN" });
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    await client.startMeeting(startArgs());
    await expect(
      client.setMute({ direction: "out", muted: "yes" as unknown as boolean }),
    ).rejects.toMatchObject({ code: "UNKNOWN" });
    await expect(client.setMute({ direction: "out", muted: true })).resolves.toBeUndefined();
  });

  it("autoDeleteDays accepts null and positive integers only", async () => {
    const { client } = mockClient();
    await client.setSetting({ key: "autoDeleteDays", value: 7 });
    expect((await client.getSettings()).autoDeleteDays).toBe(7);
    await client.setSetting({ key: "autoDeleteDays", value: null });
    expect((await client.getSettings()).autoDeleteDays).toBeNull();
    await client.setSetting({ key: "autoDeleteDays", value: 4_294_967_295 });
    await client.setSetting({ key: "autoDeleteDays", value: null });
    for (const bad of [0, -1, 1.5, Number.NaN, "7", 4_294_967_296]) {
      await expect(
        client.setSetting({ key: "autoDeleteDays", value: bad as unknown as number }),
      ).rejects.toMatchObject({ code: "INVALID_INPUT" });
    }
    expect((await client.getSettings()).autoDeleteDays).toBeNull();
  });

  it("list_meetings is newest first, including a meeting that was just stopped", async () => {
    const { client } = mockClient();
    const before = (await client.listMeetings()).map((m) => m.startedAtMs);
    expect(before).toEqual([...before].sort((a, b) => b - a));
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    const { meetingId } = await client.startMeeting(startArgs());
    await client.stopMeeting();
    const after = await client.listMeetings();
    expect(after[0].id).toBe(meetingId);
    expect(after.map((m) => m.startedAtMs)).toEqual(
      [...after.map((m) => m.startedAtMs)].sort((a, b) => b - a),
    );
  });

  it("list_notes is newest first, with a stable order inside one millisecond", async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date("2026-11-17T10:00:00Z"));
      const { client } = mockClient({ notes: [] });
      const a = await client.addNote({ text: "first", tag: null });
      const b = await client.addNote({ text: "second", tag: null });
      expect(a.id).not.toBe(b.id);
      expect((await client.listNotes()).map((n) => n.id)).toEqual([b.id, a.id]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("ids stay unique when the given sample data already uses the next counter value", async () => {
    const { client } = mockClient({
      notes: [{ id: "note_2", text: "old", tag: null, createdAtMs: 1 }],
      meetings: [{ id: "mtg_2", title: "old", startedAtMs: 1, durationSec: 1, status: "ready" }],
    });
    const note = await client.addNote({ text: "new", tag: null });
    expect(note.id).not.toBe("note_2");
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    const { meetingId } = await client.startMeeting(startArgs());
    expect(meetingId).not.toBe("mtg_2");
  });

  it("add_note limits: text up to 5000 characters after trimming, tag up to 64", async () => {
    const { client } = mockClient();
    await expect(
      client.addNote({ text: ` ${"x".repeat(5000)} `, tag: null }),
    ).resolves.toMatchObject({ text: "x".repeat(5000) });
    await expect(client.addNote({ text: "x".repeat(5001), tag: null })).rejects.toMatchObject({
      code: "INVALID_INPUT",
    });
    // Characters, not UTF-16 units: 5000 emoji are within the limit.
    await expect(client.addNote({ text: "😀".repeat(5000), tag: null })).resolves.toBeDefined();
    await expect(client.addNote({ text: "😀".repeat(5001), tag: null })).rejects.toMatchObject({
      code: "INVALID_INPUT",
    });
    await expect(client.addNote({ text: "ok", tag: "t".repeat(64) })).resolves.toMatchObject({
      tag: "t".repeat(64),
    });
    await expect(client.addNote({ text: "ok", tag: "t".repeat(65) })).rejects.toMatchObject({
      code: "INVALID_INPUT",
    });
  });

  it("add_note normalizes the tag: trim, drop leading #, lowercase; blank means no tag", async () => {
    const { client } = mockClient();
    const cases: [string | null, string | null][] = [
      ["  Follow-Up  ", "follow-up"],
      ["#follow-up", "follow-up"],
      ["##Work", "work"],
      ["  #  Idea ", "idea"],
      ["#", null],
      ["   ", null],
      [null, null],
    ];
    for (const [input, expected] of cases) {
      const note = await client.addNote({ text: "hello", tag: input });
      expect({ input, tag: note.tag }).toEqual({ input, tag: expected });
    }
  });

  it("add_note refuses a tag with whitespace between words", async () => {
    const { client } = mockClient();
    for (const tag of ["two words", "#big idea", "tab\tsplit"]) {
      await expect(client.addNote({ text: "hello", tag })).rejects.toMatchObject({
        code: "INVALID_INPUT",
      });
    }
  });
});

describe("production bundle", () => {
  it("no production source file imports the mock transport statically", () => {
    const root = fileURLToPath(new URL("..", import.meta.url));
    const files = readdirSync(root, { recursive: true, encoding: "utf8" }).filter(
      (f) =>
        /\.(ts|tsx)$/.test(f) &&
        !/\.test\.(ts|tsx)$/.test(f) &&
        !f.startsWith("shared/testing/") &&
        f !== "ipc/mockTransport.ts",
    );
    const offenders = files.filter((f) =>
      /^\s*import[^;]*from\s+["'][^"']*mockTransport["']/m.test(readFileSync(root + f, "utf8")),
    );
    expect(offenders).toEqual([]);
  });

  it("the mock is loaded on demand, inside the development-only branch", () => {
    const source = readFileSync(new URL("./client.ts", import.meta.url), "utf8");
    const branch = source.split("if (import.meta.")[1] ?? "";
    expect(branch.startsWith("env.DEV)")).toBe(true);
    expect(branch).toContain('await import("./mockTransport")');
  });
});

describe("start gate, layer 1: readiness (D-44)", () => {
  const readyModels = () => sampleModels().map((m) => ({ ...m, state: "ok" as const }));
  const status = (devices = {}) =>
    mockClient({ models: readyModels(), devices }).client.checkSetup();

  it("is ready when mic, capture, virtual microphone and the four models are ok", async () => {
    const s = await status();
    expect(s.canStart).toBe(true);
    expect(s.missingGroups).toEqual([]);
  });

  it.each([
    ["mic", "Microphone"],
    ["capture", "Meeting audio"],
    ["vmic", "Virtual microphone"],
  ] as const)("a %s that is not ok blocks start and names %s", async (id, name) => {
    for (const state of ["missing", "installing", "error"] as const) {
      const s = await status({ [id]: state });
      expect({ state, canStart: s.canStart, groups: s.missingGroups }).toEqual({
        state,
        canStart: false,
        groups: [name],
      });
    }
  });

  it("headphones never block readiness", async () => {
    const s = await status({ phones: "missing" });
    expect(s.canStart).toBe(true);
    expect(s.missingGroups).toEqual([]);
  });

  it("lists missing groups in the fixed order, with Models for any one of the four models", async () => {
    const { client } = mockClient({
      devices: { vmic: "missing", capture: "error", mic: "missing" },
    });
    const s = await client.checkSetup();
    expect(s.missingGroups).toEqual([
      "Microphone",
      "Meeting audio",
      "Virtual microphone",
      "Models",
    ]);
    for (const id of ["sr", "tr", "enV", "viV"] as const) {
      const models = readyModels().map((m) =>
        m.id === id ? { ...m, state: "missing" as const } : m,
      );
      const one = await mockClient({ models }).client.checkSetup();
      expect({ id, groups: one.missingGroups }).toEqual({ id, groups: ["Models"] });
    }
  });

  it("SETUP_INCOMPLETE names every missing group, joined by commas", async () => {
    const { client } = mockClient({ devices: { mic: "missing" } });
    await expect(client.startMeeting(startArgs())).rejects.toEqual({
      code: "SETUP_INCOMPLETE",
      message: "Finish setup first. Not turned on yet: Microphone, Models.",
    });
  });
});

describe("start gate, layer 2: per-meeting checks (D-44)", () => {
  const ready = () => mockClient({ models: sampleModels().map((m) => ({ ...m, state: "ok" })) });
  const checks = (headphonesConfirmed: boolean, meetingMicConfirmed: boolean) => ({
    headphonesConfirmed,
    meetingMicConfirmed,
  });

  it("starts when both checks are confirmed and a meeting app is chosen", async () => {
    const { client } = ready();
    await expect(client.startMeeting(startArgs())).resolves.toMatchObject({
      meetingId: expect.stringMatching(/^mtg_/),
    });
  });

  it("headphones fail when not confirmed, because the mock output is the built-in speakers", async () => {
    const { client } = ready();
    await expect(client.startMeeting(startArgs({ checks: checks(false, true) }))).rejects.toEqual({
      code: "START_CHECK_FAILED",
      message: "Confirm before starting: headphones in use.",
      details: ["headphones"],
    });
  });

  it("meetingMic fails when not confirmed", async () => {
    const { client } = ready();
    await expect(client.startMeeting(startArgs({ checks: checks(true, false) }))).rejects.toEqual({
      code: "START_CHECK_FAILED",
      message: "Confirm before starting: meeting app microphone set to Interlo.",
      details: ["meetingMic"],
    });
  });

  it.each(["", "   "])("meetingApp fails when the id is blank (%j)", async (meetingAppId) => {
    const { client } = ready();
    await expect(client.startMeeting(startArgs({ meetingAppId }))).rejects.toEqual({
      code: "START_CHECK_FAILED",
      message: "Confirm before starting: meeting app chosen.",
      details: ["meetingApp"],
    });
  });

  it("lists every failed check in the fixed order and never fails routing", async () => {
    const { client } = ready();
    await expect(
      client.startMeeting(startArgs({ meetingAppId: "", checks: checks(false, false) })),
    ).rejects.toEqual({
      code: "START_CHECK_FAILED",
      message:
        "Confirm before starting: headphones in use, meeting app microphone set to Interlo, meeting app chosen.",
      details: ["headphones", "meetingMic", "meetingApp"],
    });
  });

  it("a refused start does not start a meeting", async () => {
    const { client } = ready();
    await expect(
      client.startMeeting(startArgs({ checks: checks(false, false) })),
    ).rejects.toMatchObject({ code: "START_CHECK_FAILED" });
    await expect(client.stopMeeting()).rejects.toMatchObject({ code: "NO_MEETING" });
  });

  it("requestStart sorts START_CHECK_FAILED with the message and the failed ids", async () => {
    const { client } = ready();
    const outcome = await requestStart(client, startArgs({ checks: checks(false, true) }));
    expect(outcome).toEqual({
      kind: "checks-failed",
      message: "Confirm before starting: headphones in use.",
      details: ["headphones"],
    });
  });

  it("refuses in the order INVALID_INPUT, MEETING_RUNNING, SETUP_INCOMPLETE, START_CHECK_FAILED", async () => {
    const failing = startArgs({ meetingAppId: "", checks: checks(false, false) });
    const badMode = { ...failing, mode: "shout" as unknown as "both" };
    const code = (p: Promise<unknown>) =>
      p.then(
        () => "ok",
        (e: { code: string }) => e.code,
      );

    // Setup is incomplete and the checks fail: SETUP_INCOMPLETE comes first.
    const incomplete = mockClient().client;
    expect(await code(incomplete.startMeeting(failing))).toBe("SETUP_INCOMPLETE");
    expect(await code(incomplete.startMeeting(badMode))).toBe("INVALID_INPUT");

    // Setup is ready and the checks fail: START_CHECK_FAILED.
    const { client } = ready();
    expect(await code(client.startMeeting(failing))).toBe("START_CHECK_FAILED");
    expect(await code(client.startMeeting(badMode))).toBe("INVALID_INPUT");

    // A meeting runs: MEETING_RUNNING comes before setup and checks.
    await client.startMeeting(startArgs());
    expect(await code(client.startMeeting(failing))).toBe("MEETING_RUNNING");
    expect(await code(client.startMeeting(badMode))).toBe("INVALID_INPUT");
  });
});

describe("mock model downloads emit models://progress", () => {
  const collect = (client: ReturnType<typeof createClient>) => {
    const seen: ModelsProgressEvent[] = [];
    client.onModelsProgress((e) => seen.push(e));
    return seen;
  };
  const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

  it("emits downloading events and then ok for the model, when autoEmit is on", async () => {
    const transport = createMockTransport({ autoEmit: true });
    const client = createClient(transport);
    const seen = collect(client);
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    await settle();
    expect(seen.length).toBeGreaterThanOrEqual(3);
    expect(seen.every((e) => e.id === "enV")).toBe(true);
    expect(seen[0].state).toBe("downloading");
    expect(seen[seen.length - 1]).toMatchObject({ state: "ok" });
    const last = seen[seen.length - 1];
    expect(last.receivedBytes).toBe(last.totalBytes);
    const bytes = seen.filter((e) => e.state === "downloading").map((e) => e.receivedBytes);
    expect(bytes).toEqual([...bytes].sort((a, b) => a - b));
  });

  it("emits nothing when autoEmit is off", async () => {
    const { client } = mockClient();
    const seen = collect(client);
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    await settle();
    expect(seen).toEqual([]);
  });

  it("emits nothing for a refused download", async () => {
    const transport = createMockTransport({ autoEmit: true });
    const client = createClient(transport);
    const seen = collect(client);
    await client.downloadModel({ id: "enV", licenceAccepted: false }).catch(() => undefined);
    await settle();
    expect(seen).toEqual([]);
  });

  it("cancel_model_download stops the running events, and the model is missing again", async () => {
    const transport = createMockTransport({ autoEmit: true });
    const client = createClient(transport);
    const seen = collect(client);
    await client.downloadModel({ id: "enV", licenceAccepted: true });
    await client.cancelModelDownload({ id: "enV" });
    await settle();
    expect(seen.some((e) => e.state === "ok")).toBe(false);
    expect((await client.listModels()).find((m) => m.id === "enV")?.state).toBe("missing");
  });
});
