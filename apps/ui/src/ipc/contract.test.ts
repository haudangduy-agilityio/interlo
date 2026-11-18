// The Rust shell reads the same fixtures in apps/desktop/src-tauri/tests/ipc_contract.rs.
// Both sides must match fixtures/ipc/, so a renamed field or value fails one of the two tests.
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { createClient } from "./client";
import { createMockTransport, sampleModels } from "./mockTransport";
import {
  BANNER_KINDS,
  DIRECTIONS,
  IPC_ERROR_CODES,
  MEETING_MODES,
  MEETING_RUN_STATES,
  MEETING_STATUSES,
  MODEL_IDS,
  MODEL_PROGRESS_STATES,
  MODEL_STATES,
  SETUP_GROUPS,
  SETUP_ITEM_IDS,
  SETUP_ITEM_STATES,
  START_CHECK_IDS,
  type AppSettings,
  type BannerEvent,
  type CaptionEvent,
  type IpcError,
  type LatencyEvent,
  type MeetingIdResult,
  type MeetingSummary,
  type ModelInfo,
  type ModelsProgressEvent,
  type Note,
  type SetupItem,
  type SetupStatus,
  type StartMeetingArgs,
  type StatusEvent,
} from "./types";

const FIXTURES = fileURLToPath(new URL("../../../../fixtures/ipc/", import.meta.url));

function fixture(name: string): unknown {
  return JSON.parse(readFileSync(FIXTURES + name, "utf8"));
}

function asRecord(value: unknown): Record<string, unknown> {
  expect(typeof value).toBe("object");
  expect(value).not.toBeNull();
  return value as Record<string, unknown>;
}

function asList(value: unknown): Record<string, unknown>[] {
  expect(Array.isArray(value)).toBe(true);
  return (value as unknown[]).map(asRecord);
}

function keysOf(value: unknown): string[] {
  return Object.keys(asRecord(value)).sort();
}

/** The JSON files in a fixtures folder, sorted, so a new fixture is picked up without an edit. */
function fixtureNames(folder: string, pattern: RegExp): string[] {
  return readdirSync(FIXTURES + folder)
    .filter((f) => pattern.test(f))
    .sort();
}

// `satisfies` makes the compiler fail when a field is added to or removed from a type
// without updating this list.
const SETUP_STATUS = { items: true, canStart: true, missingGroups: true } satisfies Record<
  keyof SetupStatus,
  true
>;
const SETUP_ITEM = { id: true, group: true, state: true, detail: true } satisfies Record<
  keyof SetupItem,
  true
>;
const MODEL_INFO = {
  id: true,
  name: true,
  sizeBytes: true,
  state: true,
  licence: true,
  nonCommercial: true,
} satisfies Record<keyof ModelInfo, true>;
const MODELS_PROGRESS = {
  id: true,
  receivedBytes: true,
  totalBytes: true,
  state: true,
} satisfies Record<keyof ModelsProgressEvent, true>;
const MEETING_SUMMARY = {
  id: true,
  title: true,
  startedAtMs: true,
  durationSec: true,
  status: true,
} satisfies Record<keyof MeetingSummary, true>;
const APP_SETTINGS = { uiLanguage: true, ducking: true, autoDeleteDays: true } satisfies Record<
  keyof AppSettings,
  true
>;
const NOTE = { id: true, text: true, tag: true, createdAtMs: true } satisfies Record<
  keyof Note,
  true
>;
// `details` is optional: only the START_CHECK_FAILED fixture carries it.
const IPC_ERROR = { code: true, message: true, details: true } satisfies Record<
  keyof IpcError,
  true
>;
const START_ARGS = { mode: true, meetingAppId: true, checks: true } satisfies Record<
  keyof StartMeetingArgs,
  true
>;
const START_CHECKS = { headphonesConfirmed: true, meetingMicConfirmed: true } satisfies Record<
  keyof StartMeetingArgs["checks"],
  true
>;
const MEETING_ID = { meetingId: true } satisfies Record<keyof MeetingIdResult, true>;
const CAPTION = {
  lineId: true,
  direction: true,
  isFinal: true,
  source: true,
  target: true,
} satisfies Record<keyof CaptionEvent, true>;
const LATENCY = { p50Ms: true, p95Ms: true } satisfies Record<keyof LatencyEvent, true>;
const STATUS = { state: true } satisfies Record<keyof StatusEvent, true>;
const BANNER = { kind: true, message: true } satisfies Record<keyof BannerEvent, true>;

const fields = (shape: Record<string, true>) => Object.keys(shape).sort();
const sorted = (values: readonly string[]) => [...values].sort();
const unique = (values: readonly unknown[]) => [...new Set(values)] as string[];

/** Every value the fixtures use is a value the UI knows. */
function expectKnown(label: string, seen: readonly unknown[], known: readonly string[]) {
  for (const value of unique(seen))
    expect({ label, value, known: known.includes(value) }).toEqual({ label, value, known: true });
}

/** The fixtures use every value the UI knows (and no other), so none is left unchecked. */
function expectExactly(label: string, seen: readonly unknown[], known: readonly string[]) {
  expect({ label, values: sorted(unique(seen)) }).toEqual({ label, values: sorted(known) });
}

describe("IPC contract fixtures: fields", () => {
  it("setup status has the contract fields and known item ids", () => {
    const status = asRecord(fixture("setup_status.json"));
    expect(keysOf(status)).toEqual(fields(SETUP_STATUS));
    for (const item of asList(status.items)) {
      expect(keysOf(item)).toEqual(fields(SETUP_ITEM));
    }
  });

  it("each list and object fixture has the contract fields", () => {
    for (const model of asList(fixture("model_info_list.json")))
      expect(keysOf(model)).toEqual(fields(MODEL_INFO));
    for (const meeting of asList(fixture("meeting_summary_list.json")))
      expect(keysOf(meeting)).toEqual(fields(MEETING_SUMMARY));
    for (const note of asList(fixture("note_list.json")))
      expect(keysOf(note)).toEqual(fields(NOTE));
    expect(keysOf(fixture("app_settings.json"))).toEqual(fields(APP_SETTINGS));
    expect(keysOf(fixture("note.json"))).toEqual(fields(NOTE));
    expect(keysOf(fixture("start_meeting_result.json"))).toEqual(fields(MEETING_ID));
  });

  it("event fixtures have the contract fields", () => {
    expect(keysOf(fixture("events/caption.json"))).toEqual(fields(CAPTION));
    expect(keysOf(fixture("events/latency.json"))).toEqual(fields(LATENCY));
    expect(keysOf(fixture("events/status.json"))).toEqual(fields(STATUS));
    expect(keysOf(fixture("events/banner.json"))).toEqual(fields(BANNER));
    expect(keysOf(fixture("events/models_progress.json"))).toEqual(fields(MODELS_PROGRESS));
  });

  it("argument fixtures have the wire names of each command", () => {
    expect(keysOf(fixture("args/add_note.json"))).toEqual(["tag", "text"]);
    expect(keysOf(fixture("args/download_model.json"))).toEqual(["id", "licenceAccepted"]);
    expect(keysOf(fixture("args/cancel_model_download.json"))).toEqual(["id"]);
    expect(keysOf(fixture("args/set_mute.json"))).toEqual(["direction", "muted"]);
    expect(keysOf(fixture("args/set_setting.json"))).toEqual(["key", "value"]);
    expect(keysOf(fixture("args/start_meeting.json"))).toEqual(fields(START_ARGS));
    expect(keysOf(asRecord(fixture("args/start_meeting.json")).checks)).toEqual(
      fields(START_CHECKS),
    );
  });
});

describe("IPC contract fixtures: enum values", () => {
  const setupItems = asList(asRecord(fixture("setup_status.json")).items);
  const models = asList(fixture("model_info_list.json"));
  const meetings = asList(fixture("meeting_summary_list.json"));

  it("setup ids, groups and states are values the UI knows", () => {
    expect(sorted(unique(setupItems.map((i) => i.id)))).toEqual(sorted(SETUP_ITEM_IDS));
    expectExactly(
      "setup group",
      setupItems.map((i) => i.group),
      SETUP_GROUPS,
    );
    expectKnown(
      "setup state",
      setupItems.map((i) => i.state),
      SETUP_ITEM_STATES,
    );
  });

  it("model states and meeting statuses use exactly the values the UI knows", () => {
    expectExactly(
      "model state",
      models.map((m) => m.state),
      MODEL_STATES,
    );
    expectExactly(
      "meeting status",
      meetings.map((m) => m.status),
      MEETING_STATUSES,
    );
  });

  it("model licence fields are a string and a boolean, and the model ids are the UI's list", () => {
    for (const m of models) {
      expect(typeof m.licence).toBe("string");
      expect(typeof m.nonCommercial).toBe("boolean");
    }
    expect(models.map((m) => m.id).sort()).toEqual(sorted(MODEL_IDS));
  });

  it("model ids equal the setup item ids of the models", () => {
    const modelItemIds = setupItems.filter((i) => i.group === "models").map((i) => i.id);
    expect(models.map((m) => m.id)).toEqual(modelItemIds);
  });

  it("event values are values the UI knows", () => {
    expectKnown("direction", [asRecord(fixture("events/caption.json")).direction], DIRECTIONS);
    expectKnown("run state", [asRecord(fixture("events/status.json")).state], MEETING_RUN_STATES);
    expectKnown("banner kind", [asRecord(fixture("events/banner.json")).kind], BANNER_KINDS);
    const progress = asRecord(fixture("events/models_progress.json"));
    expectKnown("progress state", [progress.state], MODEL_PROGRESS_STATES);
    expectKnown("progress model id", [progress.id], MODEL_IDS);
    expect(Number.isInteger(progress.receivedBytes)).toBe(true);
    expect(Number.isInteger(progress.totalBytes)).toBe(true);
  });

  it("argument values are values the UI knows", () => {
    const start = asRecord(fixture("args/start_meeting.json"));
    expectKnown("mode", [start.mode], MEETING_MODES);
    expect(typeof start.meetingAppId).toBe("string");
    for (const flag of Object.values(asRecord(start.checks))) expect(typeof flag).toBe("boolean");
    expectKnown("direction", [asRecord(fixture("args/set_mute.json")).direction], DIRECTIONS);
    expectKnown(
      "model id",
      [asRecord(fixture("args/download_model.json")).id],
      models.map((m) => String(m.id)),
    );
    expect(typeof asRecord(fixture("args/download_model.json")).licenceAccepted).toBe("boolean");
    expectKnown(
      "cancelled model id",
      [asRecord(fixture("args/cancel_model_download.json")).id],
      models.map((m) => String(m.id)),
    );
    expectKnown(
      "setting key",
      [asRecord(fixture("args/set_setting.json")).key],
      Object.keys(APP_SETTINGS),
    );
  });

  it("the settings fixture uses a language and a days value the UI accepts", () => {
    const settings = asRecord(fixture("app_settings.json"));
    expectKnown("language", [settings.uiLanguage], ["en", "vi"]);
    expect(settings.autoDeleteDays === null || Number.isInteger(settings.autoDeleteDays)).toBe(
      true,
    );
  });
});

describe("IPC contract fixtures: errors", () => {
  const errorNames = fixtureNames(".", /^ipc_error_.+\.json$/);
  const errors = errorNames.map((n) => asRecord(fixture(n)));
  // A code has one main fixture named after it. Another fixture of the same code names its case.
  const EXTRA_ERROR_FIXTURES: Record<string, string> = {
    "ipc_error_licence_not_accepted.json": "INVALID_INPUT",
  };

  it("every error code has a fixture, and the codes are exactly the UI's list", () => {
    expect(sorted(unique(errors.map((e) => String(e.code))))).toEqual(sorted(IPC_ERROR_CODES));
  });

  it("each error fixture has code and message, and the file name matches the code", () => {
    errorNames.forEach((name, i) => {
      const withDetails = errors[i].code === "START_CHECK_FAILED";
      expect(keysOf(errors[i])).toEqual(withDetails ? fields(IPC_ERROR) : ["code", "message"]);
      const extra = EXTRA_ERROR_FIXTURES[name];
      if (extra) expect(errors[i].code).toBe(extra);
      else expect(name).toBe(`ipc_error_${String(errors[i].code).toLowerCase()}.json`);
    });
  });

  it("the start-check error holds failed check ids in the fixed order, without duplicates", () => {
    const error = asRecord(fixture("ipc_error_start_check_failed.json"));
    const details = error.details as string[];
    expect(details.length).toBeGreaterThan(0);
    expectKnown("check id", details, START_CHECK_IDS);
    expect(details).toEqual(START_CHECK_IDS.filter((id) => details.includes(id)));
    expect(String(error.message)).toMatch(/^Confirm before starting: .+\.$/);
  });

  it("the setup-incomplete error uses the start-gate wording", () => {
    const error = asRecord(fixture("ipc_error_setup_incomplete.json"));
    expect(String(error.message)).toMatch(/^Finish setup first\. Not turned on yet: .+\.$/);
  });
});

describe("IPC contract fixtures: rules", () => {
  it("lists are newest first", () => {
    const meetings = asList(fixture("meeting_summary_list.json")).map((m) => Number(m.startedAtMs));
    const notes = asList(fixture("note_list.json")).map((n) => Number(n.createdAtMs));
    for (const times of [meetings, notes]) {
      expect(times).toEqual([...times].sort((a, b) => b - a));
    }
  });

  it("canStart and missingGroups follow the readiness rule of docs/ipc-contract.md", () => {
    const status = asRecord(fixture("setup_status.json")) as unknown as SetupStatus;
    const ok = (id: string) => status.items.find((i) => i.id === id)?.state === "ok";
    const modelsReady = ["sr", "tr", "enV", "viV"].every(ok);
    expect(status.missingGroups).toEqual([
      ...(ok("mic") ? [] : ["Microphone"]),
      ...(ok("capture") ? [] : ["Meeting audio"]),
      ...(ok("vmic") ? [] : ["Virtual microphone"]),
      ...(modelsReady ? [] : ["Models"]),
    ]);
    expect(status.canStart).toBe(status.missingGroups.length === 0);
  });
});

describe("the mock transport follows the fixtures", () => {
  const client = (options = {}) =>
    createClient(createMockTransport({ autoEmit: false, ...options }));
  const readyModels = () => sampleModels().map((m) => ({ ...m, state: "ok" as const }));

  it("puts every setup item in the same group as the fixture, with the same ids", async () => {
    const expected = new Map(
      (asRecord(fixture("setup_status.json")).items as SetupItem[]).map((i) => [i.id, i.group]),
    );
    const status = await client().checkSetup();
    expect(status.items.map((i) => i.id).sort()).toEqual([...expected.keys()].sort());
    for (const item of status.items) {
      expect({ id: item.id, group: item.group }).toEqual({
        id: item.id,
        group: expected.get(item.id),
      });
    }
  });

  it("reports the readiness of the fixture's rule for a ready setup", async () => {
    const status = await client({ models: readyModels() }).checkSetup();
    expect(status.canStart).toBe(true);
    expect(status.missingGroups).toEqual([]);
  });

  it("lists the same model ids as the fixture", async () => {
    const ids = (await client().listModels()).map((m) => m.id);
    expect(ids).toEqual(asList(fixture("model_info_list.json")).map((m) => m.id));
  });

  it("lists models with the same fields as the fixture, and the same licence flags", async () => {
    const listed = await client().listModels();
    const expected = asList(fixture("model_info_list.json"));
    for (const model of listed) expect(keysOf(model)).toEqual(fields(MODEL_INFO));
    expect(listed.map((m) => m.nonCommercial)).toContain(true);
    expect(expected.map((m) => m.nonCommercial)).toContain(true);
  });

  it("accepts every argument fixture", async () => {
    const mock = client();
    await mock.downloadModel(
      fixture("args/download_model.json") as { id: string; licenceAccepted: boolean },
    );
    await mock.cancelModelDownload(fixture("args/cancel_model_download.json") as { id: string });
    const { meetingId } = await mock.startMeeting(
      fixture("args/start_meeting.json") as StartMeetingArgs,
    );
    expect(typeof meetingId).toBe("string");
    await mock.setMute(fixture("args/set_mute.json") as { direction: "out"; muted: boolean });
    await mock.setSetting(
      fixture("args/set_setting.json") as { key: "autoDeleteDays"; value: number },
    );
    const note = await mock.addNote(fixture("args/add_note.json") as { text: string; tag: string });
    expect(keysOf(note)).toEqual(fields(NOTE));
  });

  it("rejects with the same code and message as the error fixtures it can reproduce", async () => {
    const message = (name: string) => fixture(name) as IpcError;
    const mock = client();
    const args = fixture("args/start_meeting.json") as StartMeetingArgs;
    await expect(mock.startMeeting(args)).rejects.toEqual(
      message("ipc_error_setup_incomplete.json"),
    );
    await expect(mock.stopMeeting()).rejects.toEqual(message("ipc_error_no_meeting.json"));
    await expect(mock.downloadModel({ id: "enV", licenceAccepted: false })).rejects.toEqual(
      message("ipc_error_licence_not_accepted.json"),
    );
    await mock.downloadModel({ id: "enV", licenceAccepted: true });
    const checkFailed = message("ipc_error_start_check_failed.json");
    // The fixture's failed checks are reproduced by leaving those confirmations off.
    await expect(
      mock.startMeeting({
        ...args,
        checks: {
          headphonesConfirmed: !(checkFailed.details ?? []).includes("headphones"),
          meetingMicConfirmed: !(checkFailed.details ?? []).includes("meetingMic"),
        },
      }),
    ).rejects.toEqual(checkFailed);
    await mock.startMeeting(args);
    await expect(mock.startMeeting(args)).rejects.toEqual(
      message("ipc_error_meeting_running.json"),
    );
    await expect(mock.setSetting({ key: "autoDeleteDays", value: 0 })).rejects.toEqual(
      message("ipc_error_invalid_input.json"),
    );
  });
});

describe("the mock's progress events follow the fixture", () => {
  it("emits models://progress with the fixture's fields and a known state", async () => {
    const transport = createMockTransport({ autoEmit: true });
    const mock = createClient(transport);
    const seen: ModelsProgressEvent[] = [];
    mock.onModelsProgress((e) => seen.push(e));
    await mock.downloadModel({ id: "enV", licenceAccepted: true });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(seen.length).toBeGreaterThan(0);
    for (const e of seen) {
      expect(keysOf(e)).toEqual(fields(MODELS_PROGRESS));
      expect(MODEL_PROGRESS_STATES).toContain(e.state);
    }
  });
});
