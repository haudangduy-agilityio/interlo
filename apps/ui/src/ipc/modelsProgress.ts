import {
  MODEL_IDS,
  MODEL_PROGRESS_STATES,
  type ModelProgressState,
  type ModelsProgressEvent,
} from "./types";

/** What the screens know about model downloads, built from `models://progress` events. */
export interface ModelsProgressState {
  /** The latest event of each model. An error stays until the owner retries or dismisses it. */
  byId: Readonly<Record<string, ModelsProgressEvent>>;
  /** Counts the downloads that finished ("ok"), so a screen can load its list again. */
  finished: number;
}

export const initialModelsProgress: ModelsProgressState = { byId: {}, finished: 0 };

export type ModelsProgressAction =
  { type: "progress"; event: ModelsProgressEvent } | { type: "cleared"; id: string };

const isKnownId = (id: unknown): boolean => (MODEL_IDS as readonly unknown[]).includes(id);
const isKnownState = (state: unknown): state is ModelProgressState =>
  (MODEL_PROGRESS_STATES as readonly unknown[]).includes(state);

export function modelsProgressReducer(
  state: ModelsProgressState,
  action: ModelsProgressAction,
): ModelsProgressState {
  switch (action.type) {
    case "progress": {
      const { event } = action;
      // docs/ipc-contract.md: an id the UI does not know is ignored.
      if (!isKnownId(event.id) || !isKnownState(event.state)) return state;
      return {
        byId: { ...state.byId, [event.id]: event },
        finished: event.state === "ok" ? state.finished + 1 : state.finished,
      };
    }
    case "cleared": {
      if (!(action.id in state.byId)) return state;
      const rest = Object.entries(state.byId).filter(([id]) => id !== action.id);
      return { ...state, byId: Object.fromEntries(rest) };
    }
  }
}

/** Whole percent from 0 to 100, or null when the total size is not known. */
export function downloadPercent(event: ModelsProgressEvent): number | null {
  if (!(event.totalBytes > 0)) return null;
  const percent = Math.round((event.receivedBytes / event.totalBytes) * 100);
  return Math.min(100, Math.max(0, percent));
}

/** The model is being fetched or checked now. "ok" and "error" are not active. */
export function isActiveDownload(event: ModelsProgressEvent | undefined): boolean {
  return event?.state === "downloading" || event?.state === "verifying";
}
