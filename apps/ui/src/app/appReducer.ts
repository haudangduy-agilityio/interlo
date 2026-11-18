import type { BannerKind } from "../shared/ui";
import type { ScreenId } from "./screens";

export interface ToastState {
  id: number;
  message: string;
  kind: BannerKind;
}

export interface AppState {
  screen: ScreenId;
  /** Set while the meeting-detail screen is open. */
  meetingId: string | null;
  settingsOpen: boolean;
  toast: ToastState | null;
  nextToastId: number;
}

export const initialAppState: AppState = {
  screen: "home",
  meetingId: null,
  settingsOpen: false,
  toast: null,
  nextToastId: 1,
};

export type AppAction =
  | { type: "navigate"; screen: Exclude<ScreenId, "meeting-detail"> }
  | { type: "openMeeting"; meetingId: string }
  | { type: "openSettings" }
  | { type: "closeSettings" }
  | { type: "showToast"; message: string; kind?: BannerKind }
  | { type: "dismissToast" };

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "navigate":
      return { ...state, screen: action.screen, meetingId: null, settingsOpen: false };
    case "openMeeting":
      return {
        ...state,
        screen: "meeting-detail",
        meetingId: action.meetingId,
        settingsOpen: false,
      };
    case "openSettings":
      return { ...state, settingsOpen: true };
    case "closeSettings":
      return { ...state, settingsOpen: false };
    case "showToast":
      return {
        ...state,
        toast: { id: state.nextToastId, message: action.message, kind: action.kind ?? "info" },
        nextToastId: state.nextToastId + 1,
      };
    case "dismissToast":
      return { ...state, toast: null };
  }
}
