import { en, type MessageKey } from "../shared/messages.en";
import {
  MODEL_IDS,
  SETUP_GROUPS,
  type SetupGroup,
  type SetupItemId,
  type SetupItemState,
  type MeetingRunState,
  type StartCheckId,
} from "./types";

/** The English text of each key, for code and tests that have no language. */
function english<K extends string>(keys: Record<K, MessageKey>): Record<K, string> {
  return Object.fromEntries(
    Object.entries<MessageKey>(keys).map(([id, key]) => [id, en[key]]),
  ) as Record<K, string>;
}

/** The label of a meeting state. People see "Running", never the raw id "running". */
export const RUN_STATE_KEY: Record<MeetingRunState, MessageKey> = {
  idle: "status.idle",
  starting: "status.starting",
  running: "status.running",
  stopping: "status.stopping",
};

// Generic display names. UI text never names third-party products. Screens show
// `t(GROUP_LABEL_KEY[group])`; the English texts are in `GROUP_LABEL`.
export const GROUP_LABEL_KEY: Record<SetupGroup, MessageKey> = {
  devices: "setup.group.devices",
  virtualMicrophone: "setup.group.virtualMicrophone",
  models: "setup.group.models",
};
export const GROUP_LABEL = english(GROUP_LABEL_KEY);

export const GROUP_ORDER: readonly SetupGroup[] = SETUP_GROUPS;

export const SETUP_ITEM_LABEL_KEY: Record<SetupItemId, MessageKey> = {
  mic: "setup.item.mic",
  phones: "setup.item.phones",
  capture: "setup.item.capture",
  vmic: "setup.item.vmic",
  sr: "setup.item.sr",
  tr: "setup.item.tr",
  enV: "setup.item.enV",
  viV: "setup.item.viV",
};
export const SETUP_ITEM_LABEL = english(SETUP_ITEM_LABEL_KEY);

/**
 * The readiness rule of docs/ipc-contract.md: one list for every mode, in the order the
 * start message names them. "phones" is not here: headphones never block readiness.
 */
export const READINESS_GROUPS: readonly { name: string; ids: readonly SetupItemId[] }[] = [
  { name: "Microphone", ids: ["mic"] },
  { name: "Meeting audio", ids: ["capture"] },
  { name: "Virtual microphone", ids: ["vmic"] },
  { name: "Models", ids: MODEL_IDS },
];

/** What the owner does about a setup item that is not ok. */
const SETUP_FIX_KEY: Record<SetupItemId, MessageKey> = {
  mic: "setup.fix.mic",
  phones: "setup.fix.phones",
  capture: "setup.fix.capture",
  vmic: "setup.fix.vmic",
  sr: "setup.fix.model",
  tr: "setup.fix.model",
  enV: "setup.fix.model",
  viV: "setup.fix.model",
};

/** The message key of the fix text for a setup item that is not ok. */
export function setupFixKey(item: { id: SetupItemId; state: SetupItemState }): MessageKey {
  return item.state === "installing" ? "setup.fix.installing" : SETUP_FIX_KEY[item.id];
}

/** The English fix text. Screens show `t(setupFixKey(item))`. */
export function setupFixText(item: { id: SetupItemId; state: SetupItemState }): string {
  return en[setupFixKey(item)];
}

/** The error message the shell sends with SETUP_INCOMPLETE. The mock builds the same text. */
export function setupIncompleteMessage(groupLabels: readonly string[]): string {
  return `Finish setup first. Not turned on yet: ${groupLabels.join(", ")}.`;
}

// START_CHECK_NAME, READINESS_GROUPS and the two message builders below are the texts the shell
// sends. The mock repeats them word for word, so they stay English and are not translated.

/** The words of a failed per-meeting check inside "Confirm before starting: <list>.". */
export const START_CHECK_NAME: Record<StartCheckId, string> = {
  headphones: "headphones in use",
  meetingMic: "meeting app microphone set to Interlo",
  meetingApp: "meeting app chosen",
  routing: "capture and voice output on separate devices",
};

/** The message key of the fix step the panel shows for a failed per-meeting check. */
export const START_CHECK_FIX_KEY: Record<StartCheckId, MessageKey> = {
  headphones: "start.fix.headphones",
  meetingMic: "start.fix.meetingMic",
  meetingApp: "start.fix.meetingApp",
  routing: "start.fix.routing",
};
export const START_CHECK_FIX = english(START_CHECK_FIX_KEY);

/** The error message the shell sends with START_CHECK_FAILED. The mock builds the same text. */
export function startCheckFailedMessage(failed: readonly StartCheckId[]): string {
  return `Confirm before starting: ${failed.map((id) => START_CHECK_NAME[id]).join(", ")}.`;
}
