import { surfaceHref } from "./screens";
import { useT } from "../shared/i18n";
import type { MeetingMode } from "../ipc/types";
import { Button, Icon, useUnavailable } from "../shared/ui";
import { StartSplit } from "./StartSplit";

export interface TopBarViewProps {
  /** A meeting runs, or is being stopped. */
  running: boolean;
  /** A stop request is running. */
  stopping: boolean;
  /** A start request is waiting. */
  starting: boolean;
  onStart: (mode?: MeetingMode) => void;
  onReviewSetup: () => void;
  onStop: () => void;
  onGoLive: () => void;
}

/** The top bar of the demo: language pair, Start or Live and Stop, compact window. */
export function TopBarView(props: TopBarViewProps) {
  const t = useT();
  const pair = useUnavailable();
  const swap = useUnavailable();
  return (
    <div className="top" role="toolbar" aria-label={t("topbar.label")}>
      <div className="pair">
        <button type="button" className="pair__btn" aria-label={t("topbar.pair")} {...pair.attrs}>
          <span>
            <small>{t("topbar.speaks")}</small>
            <b>{t("topbar.vietnamese")}</b>
          </span>
          <span>
            <small>{t("topbar.hears")}</small>
            <b>{t("topbar.english")}</b>
          </span>
        </button>
        {pair.hint}
        <button type="button" className="iconbtn" aria-label={t("topbar.swap")} {...swap.attrs}>
          <Icon name="swap" size={16} />
        </button>
        {swap.hint}
      </div>
      <div className="top__grow" />
      {props.running ? (
        <>
          <button type="button" className="chip chip--live" onClick={props.onGoLive}>
            <span className="dot dot--live" aria-hidden="true" />
            {t("topbar.live")}
          </button>
          <Button variant="danger" disabled={props.stopping} onClick={props.onStop}>
            <Icon name="stop" size={14} />
            {props.stopping ? t("live.stopping") : t("topbar.stop")}
          </Button>
        </>
      ) : (
        <StartSplit
          onStart={props.onStart}
          onReviewSetup={props.onReviewSetup}
          busy={props.starting}
        />
      )}
      <a className="iconbtn" href={surfaceHref("compact-panel")} aria-label={t("topbar.compact")}>
        <Icon name="panel" size={17} />
      </a>
    </div>
  );
}
