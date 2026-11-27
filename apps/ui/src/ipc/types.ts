// IPC contract between the UI and the Rust shell. Command names are snake_case.
// The Rust side implements the same shapes; change both sides together.

// ---- Setup ----------------------------------------------------------------

// Each value list is written once. The type comes from it, and contract.test.ts checks it
// against fixtures/ipc/.
export const SETUP_ITEM_IDS = [
  "mic",
  "phones",
  "capture",
  "vmic",
  "sr",
  "tr",
  "enV",
  "viV",
] as const;
export type SetupItemId = (typeof SETUP_ITEM_IDS)[number];
export const SETUP_GROUPS = ["devices", "virtualMicrophone", "models"] as const;
export type SetupGroup = (typeof SETUP_GROUPS)[number];
export const SETUP_ITEM_STATES = ["ok", "missing", "installing", "error"] as const;
export type SetupItemState = (typeof SETUP_ITEM_STATES)[number];

export interface SetupItem {
  id: SetupItemId;
  group: SetupGroup;
  state: SetupItemState;
  detail: string;
}

export interface SetupStatus {
  items: SetupItem[];
  canStart: boolean;
  missingGroups: string[];
}

// ---- Models ---------------------------------------------------------------

export const MODEL_STATES = ["ok", "missing", "downloading", "error"] as const;
export type ModelState = (typeof MODEL_STATES)[number];

/** The ids of the models, in display order. Each is also a SetupItemId. */
export const MODEL_IDS = ["sr", "tr", "enV", "viV"] as const satisfies readonly SetupItemId[];
export type ModelId = (typeof MODEL_IDS)[number];

export interface ModelInfo {
  /** Equals the SetupItemId of the model: "sr", "tr", "enV" or "viV". */
  id: SetupItemId;
  name: string;
  sizeBytes: number;
  state: ModelState;
  /** A short licence line for the download dialog. An empty string means none is listed. */
  licence: string;
  /** The licence forbids commercial use, or the training data is not cleared for it. */
  nonCommercial: boolean;
}

export const MODEL_PROGRESS_STATES = ["downloading", "verifying", "ok", "error"] as const;
export type ModelProgressState = (typeof MODEL_PROGRESS_STATES)[number];

/** The `models://progress` payload. The counts are integers. */
export interface ModelsProgressEvent {
  id: string;
  receivedBytes: number;
  totalBytes: number;
  state: ModelProgressState;
}

// ---- Meetings -------------------------------------------------------------

export const MEETING_MODES = ["off", "listen", "speak", "both"] as const;
export type MeetingMode = (typeof MEETING_MODES)[number];
export const DIRECTIONS = ["out", "in"] as const;
export type Direction = (typeof DIRECTIONS)[number];
export const MEETING_STATUSES = ["ready", "generating", "interrupted", "failed"] as const;
export type MeetingStatus = (typeof MEETING_STATUSES)[number];

export interface MeetingSummary {
  id: string;
  title: string;
  startedAtMs: number;
  durationSec: number;
  status: MeetingStatus;
}

// ---- Settings -------------------------------------------------------------

export const UI_LANGUAGES = ["en", "vi"] as const;
export type UiLanguage = (typeof UI_LANGUAGES)[number];

export interface AppSettings {
  uiLanguage: UiLanguage;
  ducking: boolean;
  autoDeleteDays: number | null;
}
export type SettingKey = keyof AppSettings;
export type SettingValue = AppSettings[SettingKey];

// ---- Notes ----------------------------------------------------------------

export interface Note {
  id: string;
  text: string;
  tag: string | null;
  createdAtMs: number;
}

// ---- Errors ---------------------------------------------------------------

/** The only list of error codes. The runtime array and the type both come from it. */
export const IPC_ERROR_CODES = [
  "SETUP_INCOMPLETE",
  "START_CHECK_FAILED",
  "NOT_IMPLEMENTED",
  "MEETING_RUNNING",
  "NO_MEETING",
  "INVALID_INPUT",
  "START_FAILED",
] as const;
export type IpcErrorCode = (typeof IPC_ERROR_CODES)[number];

/** The per-meeting checks, in the fixed order of `IpcError.details` (docs/ipc-contract.md). */
export const START_CHECK_IDS = ["headphones", "meetingMic", "meetingApp", "routing"] as const;
export type StartCheckId = (typeof START_CHECK_IDS)[number];

/** Commands reject with this shape. */
export interface IpcError {
  code: IpcErrorCode;
  message: string;
  /** The ids of the failed checks. Only START_CHECK_FAILED carries it. */
  details?: string[];
}

// ---- Commands -------------------------------------------------------------

export type DownloadModelArgs = { id: string; licenceAccepted: boolean };
export type CancelModelDownloadArgs = { id: string };
export type StartChecks = { headphonesConfirmed: boolean; meetingMicConfirmed: boolean };
export type StartMeetingArgs = { mode: MeetingMode; meetingAppId: string; checks: StartChecks };
export type MeetingIdResult = { meetingId: string };
export type SetMuteArgs = { direction: Direction; muted: boolean };
export type SetSettingArgs = { key: SettingKey; value: SettingValue };
export type AddNoteArgs = { text: string; tag: string | null };

/** Command name to argument and result types. */
export interface CommandMap {
  check_setup: { args: undefined; result: SetupStatus };
  list_models: { args: undefined; result: ModelInfo[] };
  download_model: { args: DownloadModelArgs; result: void };
  cancel_model_download: { args: CancelModelDownloadArgs; result: void };
  start_meeting: { args: StartMeetingArgs; result: MeetingIdResult };
  stop_meeting: { args: undefined; result: MeetingIdResult };
  set_mute: { args: SetMuteArgs; result: void };
  list_meetings: { args: undefined; result: MeetingSummary[] };
  get_settings: { args: undefined; result: AppSettings };
  set_setting: { args: SetSettingArgs; result: void };
  list_notes: { args: undefined; result: Note[] };
  add_note: { args: AddNoteArgs; result: Note };
}
export type CommandName = keyof CommandMap;

// ---- Events ---------------------------------------------------------------

export interface CaptionEvent {
  lineId: string;
  direction: Direction;
  isFinal: boolean;
  source: string;
  target: string;
  /**
   * Milliseconds from the end-of-speech report (about 256 ms after speech ends), or the
   * forced cut at 15 s, until this text is ready. A whole number, 0 or more.
   */
  delayMs: number;
}

export interface LatencyEvent {
  p50Ms: number;
  p95Ms: number;
}

export const MEETING_RUN_STATES = ["idle", "starting", "running", "stopping"] as const;
export type MeetingRunState = (typeof MEETING_RUN_STATES)[number];

export interface StatusEvent {
  state: MeetingRunState;
}

export const BANNER_KINDS = ["info", "warning", "error"] as const;
export type BannerKind = (typeof BANNER_KINDS)[number];

export interface BannerEvent {
  kind: BannerKind;
  message: string;
}

export const EVENT_CAPTION = "meeting://caption";
export const EVENT_LATENCY = "meeting://latency";
export const EVENT_STATUS = "meeting://status";
export const EVENT_BANNER = "meeting://banner";
export const EVENT_MODELS_PROGRESS = "models://progress";

export interface EventMap {
  "meeting://caption": CaptionEvent;
  "meeting://latency": LatencyEvent;
  "meeting://status": StatusEvent;
  "meeting://banner": BannerEvent;
  "models://progress": ModelsProgressEvent;
}
