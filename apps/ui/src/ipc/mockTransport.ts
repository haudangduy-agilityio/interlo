import type { Transport } from "./transport";
import { READINESS_GROUPS, setupIncompleteMessage, startCheckFailedMessage } from "./labels";
import {
  MEETING_MODES,
  type AddNoteArgs,
  type AppSettings,
  type CancelModelDownloadArgs,
  type CaptionEvent,
  type DownloadModelArgs,
  type IpcError,
  type IpcErrorCode,
  type MeetingMode,
  type MeetingSummary,
  type ModelInfo,
  type ModelsProgressEvent,
  type Note,
  type SetMuteArgs,
  type SetSettingArgs,
  type SetupItem,
  type SetupItemId,
  type SetupItemState,
  type SetupStatus,
  type StartCheckId,
  type StartMeetingArgs,
} from "./types";

/** In-memory shell for the browser and for tests. All data here is sample data. */
export interface MockTransport extends Transport {
  /** Push an event to listeners, as the shell would. */
  emit(event: string, payload: unknown): void;
}

/** The device items a test can put in another state. Default: all ok. */
export type DeviceItemId = "mic" | "phones" | "capture" | "vmic";

export interface MockOptions {
  meetings?: MeetingSummary[];
  notes?: Note[];
  models?: ModelInfo[];
  devices?: Partial<Record<DeviceItemId, SetupItemState>>;
  settings?: AppSettings;
  /** Emit sample status, caption and model progress events. Default true. */
  autoEmit?: boolean;
}

const HOUR = 3_600_000;
const BASE_TIME = Date.UTC(2026, 10, 16, 9, 0, 0);

export function sampleMeetings(): MeetingSummary[] {
  return [
    {
      id: "mtg_1",
      title: "Weekly planning",
      startedAtMs: BASE_TIME - 26 * HOUR,
      durationSec: 2710,
      status: "ready",
    },
    {
      id: "mtg_2",
      title: "Vendor call",
      startedAtMs: BASE_TIME - 5 * HOUR,
      durationSec: 1260,
      status: "generating",
    },
    {
      id: "mtg_3",
      title: "Design review",
      startedAtMs: BASE_TIME - 50 * HOUR,
      durationSec: 3540,
      status: "failed",
    },
    {
      id: "mtg_4",
      title: "Customer interview",
      startedAtMs: BASE_TIME - 75 * HOUR,
      durationSec: 600,
      status: "interrupted",
    },
  ];
}

const OPEN_LICENCE = "Permissive open licence";

export function sampleModels(): ModelInfo[] {
  return [
    {
      id: "sr",
      name: "Speech recognition",
      sizeBytes: 780_000_000,
      state: "ok",
      licence: OPEN_LICENCE,
      nonCommercial: false,
    },
    {
      id: "tr",
      name: "Translation",
      sizeBytes: 1_200_000_000,
      state: "ok",
      licence: "Open licence, trained on data not cleared for commercial use",
      nonCommercial: true,
    },
    {
      id: "enV",
      name: "English voice",
      sizeBytes: 90_000_000,
      state: "missing",
      licence: OPEN_LICENCE,
      nonCommercial: false,
    },
    {
      id: "viV",
      name: "Vietnamese voice",
      sizeBytes: 85_000_000,
      state: "ok",
      licence: OPEN_LICENCE,
      nonCommercial: false,
    },
  ];
}

export function sampleNotes(): Note[] {
  return [
    {
      id: "note_1",
      text: "Ask about the rollout date.",
      tag: "follow-up",
      createdAtMs: BASE_TIME - 2 * HOUR,
    },
    {
      id: "note_2",
      text: "Share the summary with the team.",
      tag: null,
      createdAtMs: BASE_TIME - 30 * HOUR,
    },
  ];
}

const NOTE_TEXT_MAX = 5000;
const NOTE_TAG_MAX = 64;

const MAX_SETTING_DAYS = 4_294_967_295;

const charCount = (text: string): number => [...text].length;

/**
 * The contract's tag rule (docs/ipc-contract.md): trim, drop leading "#", lowercase.
 * A blank or "#"-only tag is no tag. Whitespace between characters is refused.
 */
function normalizeTag(tag: string | null | undefined): { tag: string | null } | "invalid" {
  const bare = (tag ?? "").trim().replace(/^#+/, "").trim();
  if (bare.length === 0) return { tag: null };
  if (/\s/.test(bare)) return "invalid";
  return { tag: bare.toLowerCase() };
}

function newestFirst<T>(items: readonly T[], timeOf: (item: T) => number): T[] {
  // Array.prototype.sort is stable: items with the same time keep their given order.
  return [...items].sort((a, b) => timeOf(b) - timeOf(a));
}

const MODEL_ITEMS: readonly { id: SetupItemId; detail: string }[] = [
  { id: "sr", detail: "Speech recognition model" },
  { id: "tr", detail: "Translation model" },
  { id: "enV", detail: "English voice model" },
  { id: "viV", detail: "Vietnamese voice model" },
];

const unknownModelMessage = (id: unknown): string => `Unknown model: ${String(id ?? "")}.`;

function fail(code: IpcErrorCode, message: string, details?: string[]): Promise<never> {
  const error: IpcError = details ? { code, message, details } : { code, message };
  return Promise.reject(error);
}

export function createMockTransport(options: MockOptions = {}): MockTransport {
  const models = new Map<string, ModelInfo>(
    (options.models ?? sampleModels()).map((m) => [m.id, { ...m }]),
  );
  let meetings = (options.meetings ?? sampleMeetings()).map((m) => ({ ...m }));
  let notes = (options.notes ?? sampleNotes()).map((n) => ({ ...n }));
  let settings: AppSettings = {
    ...(options.settings ?? { uiLanguage: "en", ducking: true, autoDeleteDays: null }),
  };
  const autoEmit = options.autoEmit ?? true;
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  let runningMeetingId: string | null = null;
  let meetingCounter = meetings.length;
  let noteCounter = notes.length;

  /** The next id with this prefix that no existing item uses. */
  function nextId(prefix: string, counter: number, taken: ReadonlySet<string>): [string, number] {
    let n = counter + 1;
    while (taken.has(`${prefix}_${n}`)) n += 1;
    return [`${prefix}_${n}`, n];
  }

  function emit(event: string, payload: unknown): void {
    listeners.get(event)?.forEach((cb) => cb(payload));
  }

  function modelItemState(id: string): SetupItem["state"] {
    const state = models.get(id)?.state ?? "missing";
    if (state === "downloading") return "installing";
    return state;
  }

  function deviceState(id: DeviceItemId): SetupItemState {
    return options.devices?.[id] ?? "ok";
  }

  function setupStatus(): SetupStatus {
    const items: SetupItem[] = [
      {
        id: "mic",
        group: "devices",
        state: deviceState("mic"),
        detail: "Default microphone found.",
      },
      { id: "phones", group: "devices", state: deviceState("phones"), detail: "Headphones found." },
      { id: "capture", group: "devices", state: deviceState("capture"), detail: "Installed." },
      { id: "vmic", group: "virtualMicrophone", state: deviceState("vmic"), detail: "Installed." },
      ...MODEL_ITEMS.map((m): SetupItem => {
        const state = modelItemState(m.id);
        return {
          id: m.id,
          group: "models",
          state,
          detail: state === "ok" ? "Ready." : `${m.detail} is not ready.`,
        };
      }),
    ];
    // One rule for every mode (docs/ipc-contract.md). "phones" never blocks readiness.
    const notOk = new Set<SetupItemId>(items.filter((i) => i.state !== "ok").map((i) => i.id));
    const missingGroups = READINESS_GROUPS.filter((g) => g.ids.some((id) => notOk.has(id))).map(
      (g) => g.name,
    );
    return { items, canStart: missingGroups.length === 0, missingGroups };
  }

  /** The failed per-meeting checks, in the contract's fixed order. */
  function failedChecks(args: StartMeetingArgs): StartCheckId[] {
    const failed: StartCheckId[] = [];
    // The mock's output device is always the built-in speakers.
    if (args.checks?.headphonesConfirmed !== true) failed.push("headphones");
    if (args.checks?.meetingMicConfirmed !== true) failed.push("meetingMic");
    if (typeof args.meetingAppId !== "string" || args.meetingAppId.trim().length === 0)
      failed.push("meetingApp");
    // "routing" is a fact from the shell. The mock never reports a shared virtual device.
    return failed;
  }

  function sampleCaptions(direction: CaptionEvent["direction"]): CaptionEvent[] {
    const source =
      direction === "in" ? "We can ship this on Friday." : "I will send the notes today.";
    const target =
      direction === "in" ? "Chúng ta có thể phát hành vào thứ Sáu." : "Hôm nay tôi sẽ gửi ghi chú.";
    return [
      {
        lineId: `line_${direction}_1`,
        direction,
        isFinal: false,
        source: source.slice(0, 12),
        target: "",
      },
      { lineId: `line_${direction}_1`, direction, isFinal: true, source, target },
    ];
  }

  function startMeeting(args: StartMeetingArgs | undefined): Promise<{ meetingId: string }> {
    const mode = args?.mode as MeetingMode | undefined;
    if (!mode || !(MEETING_MODES as readonly string[]).includes(mode))
      return fail("INVALID_INPUT", "Mode must be off, listen, speak or both.");
    if (runningMeetingId) return fail("MEETING_RUNNING", "A meeting is already running.");
    const setup = setupStatus();
    if (!setup.canStart)
      return fail("SETUP_INCOMPLETE", setupIncompleteMessage(setup.missingGroups));
    const failed = failedChecks(args as StartMeetingArgs);
    if (failed.length > 0)
      return fail("START_CHECK_FAILED", startCheckFailedMessage(failed), failed);
    const [meetingId, counter] = nextId("mtg", meetingCounter, new Set(meetings.map((m) => m.id)));
    meetingCounter = counter;
    runningMeetingId = meetingId;
    if (autoEmit) {
      queueMicrotask(() => {
        emit("meeting://status", { state: "starting" });
        emit("meeting://status", { state: "running" });
        emit("meeting://latency", { p50Ms: 420, p95Ms: 780 });
      });
      setTimeout(() => {
        if (runningMeetingId !== meetingId) return;
        for (const e of sampleCaptions("in")) emit("meeting://caption", e);
        for (const e of sampleCaptions("out")) emit("meeting://caption", e);
      }, 0);
    }
    return Promise.resolve({ meetingId });
  }

  function stopMeeting(): Promise<{ meetingId: string }> {
    if (!runningMeetingId) return fail("NO_MEETING", "No meeting is running.");
    const meetingId = runningMeetingId;
    runningMeetingId = null;
    meetings = newestFirst(
      [
        {
          id: meetingId,
          title: "Untitled meeting",
          startedAtMs: Date.now(),
          durationSec: 0,
          status: "generating",
        },
        ...meetings,
      ],
      (m) => m.startedAtMs,
    );
    emit("meeting://status", { state: "stopping" });
    emit("meeting://status", { state: "idle" });
    return Promise.resolve({ meetingId });
  }

  function setMute(args: SetMuteArgs | undefined): Promise<void> {
    // The shell's bridge refuses a wrong shape before the command runs, with a plain string.
    if (
      !args ||
      (args.direction !== "out" && args.direction !== "in") ||
      typeof args.muted !== "boolean"
    ) {
      return Promise.reject("Invalid arguments for set_mute: direction must be out or in.");
    }
    if (!runningMeetingId) return fail("NO_MEETING", "No meeting is running.");
    return Promise.resolve();
  }

  function setSetting(args: SetSettingArgs | undefined): Promise<void> {
    if (!args) return fail("INVALID_INPUT", "Missing setting.");
    const { key, value } = args;
    if (key === "uiLanguage" && (value === "en" || value === "vi"))
      settings = { ...settings, uiLanguage: value };
    else if (key === "ducking" && typeof value === "boolean")
      settings = { ...settings, ducking: value };
    else if (
      key === "autoDeleteDays" &&
      (value === null ||
        (typeof value === "number" &&
          Number.isInteger(value) &&
          value >= 1 &&
          value <= MAX_SETTING_DAYS))
    ) {
      settings = { ...settings, autoDeleteDays: value };
    } else return fail("INVALID_INPUT", `Invalid value for setting: ${String(key)}.`);
    return Promise.resolve();
  }

  /** The timers of the sample progress events that have not run yet, by model id. */
  const runningDownloads = new Map<string, ReturnType<typeof setTimeout>[]>();

  function emitSampleProgress(model: ModelInfo): void {
    const total = model.sizeBytes;
    const steps: ModelsProgressEvent[] = [
      { id: model.id, receivedBytes: 0, totalBytes: total, state: "downloading" },
      {
        id: model.id,
        receivedBytes: Math.floor(total / 2),
        totalBytes: total,
        state: "downloading",
      },
      { id: model.id, receivedBytes: total, totalBytes: total, state: "verifying" },
      { id: model.id, receivedBytes: total, totalBytes: total, state: "ok" },
    ];
    const timers = steps.map((step, i) =>
      setTimeout(() => {
        emit("models://progress", step);
        if (i === steps.length - 1) runningDownloads.delete(model.id);
      }, i),
    );
    runningDownloads.set(model.id, timers);
  }

  function downloadModel(args: DownloadModelArgs | undefined): Promise<void> {
    const model = args ? models.get(args.id) : undefined;
    // The contract's order: the id first, then the licence.
    if (!model) return fail("INVALID_INPUT", unknownModelMessage(args?.id));
    if (args?.licenceAccepted !== true) return fail("INVALID_INPUT", "Accept the licence first.");
    // The state is final at once in the mock. The real shell reports "downloading" first.
    model.state = "ok";
    if (autoEmit) emitSampleProgress(model);
    return Promise.resolve();
  }

  function cancelModelDownload(args: CancelModelDownloadArgs | undefined): Promise<void> {
    const model = args ? models.get(args.id) : undefined;
    if (!model) return fail("INVALID_INPUT", unknownModelMessage(args?.id));
    const timers = runningDownloads.get(model.id);
    if (timers) {
      timers.forEach(clearTimeout);
      runningDownloads.delete(model.id);
      model.state = "missing";
    }
    return Promise.resolve();
  }

  function addNote(args: AddNoteArgs | undefined): Promise<Note> {
    const text = typeof args?.text === "string" ? args.text.trim() : "";
    if (text.length === 0) return fail("INVALID_INPUT", "A note needs some text.");
    if (charCount(text) > NOTE_TEXT_MAX)
      return fail("INVALID_INPUT", `A note can have at most ${NOTE_TEXT_MAX} characters.`);
    const normalized = normalizeTag(args?.tag);
    if (normalized === "invalid")
      return fail("INVALID_INPUT", "A tag is one word, without spaces.");
    const tag = normalized.tag;
    if (tag !== null && charCount(tag) > NOTE_TAG_MAX)
      return fail("INVALID_INPUT", `A tag can have at most ${NOTE_TAG_MAX} characters.`);
    const [id, counter] = nextId("note", noteCounter, new Set(notes.map((n) => n.id)));
    noteCounter = counter;
    const note: Note = { id, text, tag, createdAtMs: Date.now() };
    notes = [note, ...notes];
    return Promise.resolve({ ...note });
  }

  function handle(cmd: string, args: Record<string, unknown> | undefined): Promise<unknown> {
    switch (cmd) {
      case "check_setup":
        return Promise.resolve(setupStatus());
      case "list_models":
        return Promise.resolve([...models.values()].map((m) => ({ ...m })));
      case "download_model":
        return downloadModel(args as DownloadModelArgs | undefined);
      case "cancel_model_download":
        return cancelModelDownload(args as CancelModelDownloadArgs | undefined);
      case "start_meeting":
        return startMeeting(args as StartMeetingArgs | undefined);
      case "stop_meeting":
        return stopMeeting();
      case "set_mute":
        return setMute(args as SetMuteArgs | undefined);
      case "list_meetings":
        return Promise.resolve(newestFirst(meetings, (m) => m.startedAtMs).map((m) => ({ ...m })));
      case "get_settings":
        return Promise.resolve({ ...settings });
      case "set_setting":
        return setSetting(args as SetSettingArgs | undefined);
      case "list_notes":
        return Promise.resolve(newestFirst(notes, (n) => n.createdAtMs).map((n) => ({ ...n })));
      case "add_note":
        return addNote(args as AddNoteArgs | undefined);
      default:
        return fail("NOT_IMPLEMENTED", `Command ${cmd} is not implemented.`);
    }
  }

  return {
    invoke: <T>(cmd: string, args?: Record<string, unknown>) => handle(cmd, args) as Promise<T>,
    listen(event, cb) {
      let set = listeners.get(event);
      if (!set) {
        set = new Set();
        listeners.set(event, set);
      }
      set.add(cb);
      return () => {
        set.delete(cb);
      };
    },
    emit,
  };
}
