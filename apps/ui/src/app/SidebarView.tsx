import { useId } from "react";
import { useT } from "../shared/i18n";
import { Button, Icon, KeyCap, Meter, useUnavailable } from "../shared/ui";
import { NAV_ITEMS, type NavItem } from "./screens";

/** What the Local-only card says about the setup. */
export type LocalSetupState = "ready" | "missing" | "checking" | "unknown";

export interface SidebarViewProps {
  active: NavItem["id"];
  onNavigate: (id: NavItem["id"]) => void;
  onOpenSettings: () => void;
  /** Setup items that are not ready, or null while the check loads or when it failed. */
  setupCount: number | null;
  setupState: LocalSetupState;
  meetingRunning: boolean;
  /** The page behind the settings sheet cannot be reached. */
  inert: boolean;
}

const SETUP_LINE = {
  ready: "shell.localReady",
  missing: "shell.localMissing",
  checking: "shell.localChecking",
  unknown: "shell.localUnknown",
} as const;

function NavButton({
  item,
  current,
  count,
  onNavigate,
}: {
  item: NavItem;
  current: boolean;
  count: number | null;
  onNavigate: SidebarViewProps["onNavigate"];
}) {
  const t = useT();
  const hintId = useId();
  const showCount = count !== null && count > 0;
  return (
    <>
      <button
        type="button"
        className="nav__item"
        aria-current={current ? "page" : undefined}
        aria-describedby={showCount ? hintId : undefined}
        onClick={() => onNavigate(item.id)}
      >
        <Icon name={item.icon} size={17} />
        {t(item.labelKey)}
        {showCount ? (
          <span className="chip" aria-hidden="true">
            {count}
          </span>
        ) : null}
      </button>
      {showCount ? (
        <span id={hintId} className="sr-only">
          {t("shell.setupBadge", { count })}
        </span>
      ) : null}
    </>
  );
}

/** The sidebar of the demo: brand, search, navigation, the Local-only card and Settings. */
export function SidebarView(props: SidebarViewProps) {
  const t = useT();
  const search = useUnavailable();
  const meter = useUnavailable();
  return (
    <aside className="sb" inert={props.inert}>
      <div className="brand">
        <img src="/interlo-icon.svg" width={26} height={26} alt="" />
        Interlo
      </div>
      <button type="button" className="sb__search" {...search.attrs}>
        <Icon name="search" size={15} />
        <span>{t("shell.search")}</span>
        <KeyCap size="sm">⌘K</KeyCap>
      </button>
      {search.hint}
      <nav className="nav" aria-label={t("nav.main")}>
        {NAV_ITEMS.map((item) => (
          <NavButton
            key={item.id}
            item={item}
            current={props.active === item.id}
            count={item.id === "setup" ? props.setupCount : null}
            onNavigate={props.onNavigate}
          />
        ))}
      </nav>
      <div className="sb__foot">
        <div className="localcard">
          <div className="localcard__title">
            <span className={props.meetingRunning ? "dot dot--live" : "dot"} aria-hidden="true" />
            {props.meetingRunning ? t("shell.meetingRunning") : t("shell.localOnly")}
          </div>
          <small>
            {props.meetingRunning ? t("shell.meetingRunningNote") : t(SETUP_LINE[props.setupState])}
          </small>
          <Meter label={t("shell.memory")} unavailable />
          {meter.hint}
        </div>
        <Button onClick={props.onOpenSettings}>{t("nav.settings")}</Button>
      </div>
    </aside>
  );
}
