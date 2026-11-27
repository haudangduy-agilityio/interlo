// Runtime checks for what the shell sends. The shapes come from docs/ipc-contract.md and
// fixtures/ipc/. The UI trusts nothing it did not check: a payload that fails here is dropped
// (events) or becomes a UI error (command results) before any screen reads it.
import {
  BANNER_KINDS,
  DIRECTIONS,
  MEETING_RUN_STATES,
  MEETING_STATUSES,
  MODEL_PROGRESS_STATES,
  MODEL_STATES,
  SETUP_GROUPS,
  SETUP_ITEM_IDS,
  SETUP_ITEM_STATES,
  UI_LANGUAGES,
  type AppSettings,
  type BannerEvent,
  type CaptionEvent,
  type CommandMap,
  type CommandName,
  type EventMap,
  type LatencyEvent,
  type MeetingIdResult,
  type MeetingSummary,
  type ModelInfo,
  type ModelsProgressEvent,
  type Note,
  type SetupItem,
  type SetupStatus,
  type StatusEvent,
} from "./types";

type Rec = Record<string, unknown>;

const isRecord = (v: unknown): v is Rec => typeof v === "object" && v !== null && !Array.isArray(v);
const isString = (v: unknown): v is string => typeof v === "string";
const isBoolean = (v: unknown): v is boolean => typeof v === "boolean";
const isNumber = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const isCount = (v: unknown): v is number => isNumber(v) && Number.isInteger(v) && v >= 0;
const isOneOf = <T extends string>(list: readonly T[], v: unknown): v is T =>
  typeof v === "string" && (list as readonly string[]).includes(v);
const isListOf = <T>(guard: (v: unknown) => v is T, v: unknown): v is T[] =>
  Array.isArray(v) && v.every(guard);

// ---- Events ---------------------------------------------------------------

export const isCaptionEvent = (v: unknown): v is CaptionEvent =>
  isRecord(v) &&
  isString(v.lineId) &&
  isOneOf(DIRECTIONS, v.direction) &&
  isBoolean(v.isFinal) &&
  isString(v.source) &&
  isString(v.target) &&
  isCount(v.delayMs);

export const isLatencyEvent = (v: unknown): v is LatencyEvent =>
  isRecord(v) && isNumber(v.p50Ms) && isNumber(v.p95Ms);

export const isStatusEvent = (v: unknown): v is StatusEvent =>
  isRecord(v) && isOneOf(MEETING_RUN_STATES, v.state);

export const isBannerEvent = (v: unknown): v is BannerEvent =>
  isRecord(v) && isOneOf(BANNER_KINDS, v.kind) && isString(v.message);

/** An id the UI does not know is still valid here: the reducer ignores it (docs/ipc-contract.md). */
export const isModelsProgressEvent = (v: unknown): v is ModelsProgressEvent =>
  isRecord(v) &&
  isString(v.id) &&
  isNumber(v.receivedBytes) &&
  isNumber(v.totalBytes) &&
  isOneOf(MODEL_PROGRESS_STATES, v.state);

export const EVENT_GUARDS: { [E in keyof EventMap]: (v: unknown) => v is EventMap[E] } = {
  "meeting://caption": isCaptionEvent,
  "meeting://latency": isLatencyEvent,
  "meeting://status": isStatusEvent,
  "meeting://banner": isBannerEvent,
  "models://progress": isModelsProgressEvent,
};

// ---- Command results ------------------------------------------------------

const isSetupItem = (v: unknown): v is SetupItem =>
  isRecord(v) &&
  isOneOf(SETUP_ITEM_IDS, v.id) &&
  isOneOf(SETUP_GROUPS, v.group) &&
  isOneOf(SETUP_ITEM_STATES, v.state) &&
  isString(v.detail);

export const isSetupStatus = (v: unknown): v is SetupStatus =>
  isRecord(v) &&
  isListOf(isSetupItem, v.items) &&
  isBoolean(v.canStart) &&
  isListOf(isString, v.missingGroups);

const isModelInfo = (v: unknown): v is ModelInfo =>
  isRecord(v) &&
  isOneOf(SETUP_ITEM_IDS, v.id) &&
  isString(v.name) &&
  isNumber(v.sizeBytes) &&
  isOneOf(MODEL_STATES, v.state) &&
  isString(v.licence) &&
  isBoolean(v.nonCommercial);

const isMeetingSummary = (v: unknown): v is MeetingSummary =>
  isRecord(v) &&
  isString(v.id) &&
  isString(v.title) &&
  isNumber(v.startedAtMs) &&
  isNumber(v.durationSec) &&
  isOneOf(MEETING_STATUSES, v.status);

export const isAppSettings = (v: unknown): v is AppSettings =>
  isRecord(v) &&
  isOneOf(UI_LANGUAGES, v.uiLanguage) &&
  isBoolean(v.ducking) &&
  (v.autoDeleteDays === null || isNumber(v.autoDeleteDays));

const isNote = (v: unknown): v is Note =>
  isRecord(v) &&
  isString(v.id) &&
  isString(v.text) &&
  (v.tag === null || isString(v.tag)) &&
  isNumber(v.createdAtMs);

const isMeetingIdResult = (v: unknown): v is MeetingIdResult =>
  isRecord(v) && isString(v.meetingId);

/** A command that returns nothing sends JSON null (or nothing, in the mock). */
const isNothing = (v: unknown): v is undefined => v === undefined || v === null;

export const RESULT_GUARDS: {
  [K in CommandName]: (v: unknown) => v is CommandMap[K]["result"];
} = {
  check_setup: isSetupStatus,
  list_models: (v): v is ModelInfo[] => isListOf(isModelInfo, v),
  download_model: isNothing,
  cancel_model_download: isNothing,
  start_meeting: isMeetingIdResult,
  stop_meeting: isMeetingIdResult,
  set_mute: isNothing,
  list_meetings: (v): v is MeetingSummary[] => isListOf(isMeetingSummary, v),
  get_settings: isAppSettings,
  set_setting: isNothing,
  list_notes: (v): v is Note[] => isListOf(isNote, v),
  add_note: isNote,
};
