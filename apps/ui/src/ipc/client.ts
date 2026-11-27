import { t } from "../shared/i18n";
import { toUiError, type UiError } from "./errors";
import { EVENT_GUARDS, RESULT_GUARDS } from "./guards";
import { createTauriTransport, isTauriShell, tauriApi } from "./tauriTransport";
import type { Transport } from "./transport";
import {
  EVENT_BANNER,
  EVENT_CAPTION,
  EVENT_LATENCY,
  EVENT_MODELS_PROGRESS,
  EVENT_STATUS,
  type AppSettings,
  type BannerEvent,
  type CancelModelDownloadArgs,
  type CaptionEvent,
  type CommandMap,
  type CommandName,
  type Direction,
  type DownloadModelArgs,
  type EventMap,
  type LatencyEvent,
  type MeetingSummary,
  type ModelInfo,
  type ModelsProgressEvent,
  type Note,
  type SettingKey,
  type SettingValue,
  type SetupStatus,
  type StartMeetingArgs,
  type StatusEvent,
} from "./types";

export type TransportKind = "tauri" | "mock";

export interface Client {
  readonly kind: TransportKind;
  readonly canDownloadModels: boolean;
  checkSetup(): Promise<SetupStatus>;
  listModels(): Promise<ModelInfo[]>;
  downloadModel(args: DownloadModelArgs): Promise<void>;
  cancelModelDownload(args: CancelModelDownloadArgs): Promise<void>;
  startMeeting(args: StartMeetingArgs): Promise<{ meetingId: string }>;
  stopMeeting(): Promise<{ meetingId: string }>;
  setMute(args: { direction: Direction; muted: boolean }): Promise<void>;
  listMeetings(): Promise<MeetingSummary[]>;
  getSettings(): Promise<AppSettings>;
  setSetting(args: { key: SettingKey; value: SettingValue }): Promise<void>;
  listNotes(): Promise<Note[]>;
  addNote(args: { text: string; tag: string | null }): Promise<Note>;
  onCaption(cb: (e: CaptionEvent) => void): () => void;
  onLatency(cb: (e: LatencyEvent) => void): () => void;
  onStatus(cb: (e: StatusEvent) => void): () => void;
  onBanner(cb: (e: BannerEvent) => void): () => void;
  onModelsProgress(cb: (e: ModelsProgressEvent) => void): () => void;
}

/**
 * One typed function per command. Rejections are IpcError objects. A result that does not match
 * the contract is rejected as a UI error, and an event that does not match is dropped: no screen
 * ever reads a payload that was not checked. The first dropped event shows one warning banner
 * (through the same listeners as `meeting://banner`), so a broken shell is visible but not noisy.
 */
export function createClient(transport: Transport, kind: TransportKind = "mock"): Client {
  const bannerListeners = new Set<(e: BannerEvent) => void>();
  let reportedInvalidEvent = false;

  function reportInvalidEvent(): void {
    if (reportedInvalidEvent) return;
    reportedInvalidEvent = true;
    const banner: BannerEvent = { kind: "warning", message: t("error.invalidEvent") };
    bannerListeners.forEach((cb) => cb(banner));
  }

  function call<K extends CommandName>(
    cmd: K,
    args?: CommandMap[K]["args"],
  ): Promise<CommandMap[K]["result"]> {
    return transport
      .invoke<unknown>(cmd, args as Record<string, unknown> | undefined)
      .catch((e: unknown) => {
        throw toUiError(e);
      })
      .then((result) => {
        if (!RESULT_GUARDS[cmd](result)) {
          const error: UiError = { code: "UNKNOWN", message: t("error.invalidResult") };
          throw error;
        }
        // A command with no result sends null; the UI sees undefined.
        return result ?? undefined;
      }) as Promise<CommandMap[K]["result"]>;
  }
  function on<E extends keyof EventMap>(event: E, cb: (payload: EventMap[E]) => void): () => void {
    const isValid = EVENT_GUARDS[event];
    return transport.listen(event, (payload) => {
      if (isValid(payload)) cb(payload);
      else reportInvalidEvent();
    });
  }
  function onBanner(cb: (e: BannerEvent) => void): () => void {
    bannerListeners.add(cb);
    const off = on(EVENT_BANNER, cb);
    return () => {
      bannerListeners.delete(cb);
      off();
    };
  }
  return {
    kind,
    // The desktop shell has no download client yet (#41).
    canDownloadModels: kind === "mock",
    checkSetup: () => call("check_setup"),
    listModels: () => call("list_models"),
    downloadModel: (args) => call("download_model", args),
    cancelModelDownload: (args) => call("cancel_model_download", args),
    startMeeting: (args) => call("start_meeting", args),
    stopMeeting: () => call("stop_meeting"),
    setMute: (args) => call("set_mute", args),
    listMeetings: () => call("list_meetings"),
    getSettings: () => call("get_settings"),
    setSetting: (args) => call("set_setting", args),
    listNotes: () => call("list_notes"),
    addNote: (args) => call("add_note", args),
    onCaption: (cb) => on(EVENT_CAPTION, cb),
    onLatency: (cb) => on(EVENT_LATENCY, cb),
    onStatus: (cb) => on(EVENT_STATUS, cb),
    onBanner,
    onModelsProgress: (cb) => on(EVENT_MODELS_PROGRESS, cb),
  };
}

/**
 * The desktop shell when its global exists. In development, with no shell, the in-memory mock.
 * The mock is loaded on demand and only behind `import.meta.env.DEV`, so a production build
 * drops it: sample data never ships. Without the shell, a production build refuses to start.
 */
export async function selectTransport(): Promise<{ transport: Transport; kind: TransportKind }> {
  if (isTauriShell()) {
    return { transport: createTauriTransport(tauriApi), kind: "tauri" };
  }
  if (import.meta.env.DEV) {
    const { createMockTransport } = await import("./mockTransport");
    return { transport: createMockTransport(), kind: "mock" };
  }
  throw new Error(t("error.shellNotConnected"));
}

export async function createDefaultClient(): Promise<Client> {
  const { transport, kind } = await selectTransport();
  return createClient(transport, kind);
}
