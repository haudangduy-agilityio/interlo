import { en, type MessageKey } from "../shared/messages.en";
import type { IconName } from "../shared/ui";

export const SCREEN_IDS = [
  "home",
  "live",
  "meetings",
  "meeting-detail",
  "workspace",
  "setup",
] as const;
export type ScreenId = (typeof SCREEN_IDS)[number];

export interface NavItem {
  id: Exclude<ScreenId, "meeting-detail">;
  /** Screens show `t(labelKey)`. */
  labelKey: MessageKey;
  /** The English text of `labelKey`. */
  label: string;
  icon: IconName;
}

const navItem = (id: NavItem["id"], labelKey: MessageKey, icon: IconName): NavItem => ({
  id,
  labelKey,
  label: en[labelKey],
  icon,
});

export const NAV_ITEMS: readonly NavItem[] = [
  navItem("home", "nav.home", "home"),
  navItem("live", "nav.live", "live"),
  navItem("meetings", "nav.meetings", "doc"),
  navItem("workspace", "nav.workspace", "folder"),
  navItem("setup", "nav.setup", "setup"),
];

/** The nav entry that stays highlighted while a screen is open. */
export function navIdFor(screen: ScreenId): NavItem["id"] {
  return screen === "meeting-detail" ? "meetings" : screen;
}

// Floating surfaces run in their own small windows. They are chosen by the URL hash: #surface=<id>.
export const SURFACE_IDS = ["quick-actions", "subtitle-bar", "compact-panel"] as const;
export type SurfaceId = (typeof SURFACE_IDS)[number];

export function parseSurface(hash: string): SurfaceId | null {
  const match = /^#surface=([a-z-]+)$/.exec(hash);
  if (!match) return null;
  return (SURFACE_IDS as readonly string[]).includes(match[1]) ? (match[1] as SurfaceId) : null;
}

export function surfaceHref(id: SurfaceId): string {
  return `#surface=${id}`;
}
