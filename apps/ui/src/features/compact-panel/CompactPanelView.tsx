import { RUN_STATE_KEY } from "../../ipc/labels";
import { isPaused, type SessionState } from "../../ipc/meetingSession";
import type { Direction } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import {
  ActionErrorBanner,
  Banner,
  Button,
  LoadingState,
  type ActionErrorBannerProps,
} from "../../shared/ui";
import { MuteControls } from "../live";

export interface CompactPanelViewProps {
  session: SessionState;
  /** The last refused mute, pause or stop. */
  error?: ActionErrorBannerProps;
  onToggleMute: (direction: Direction) => void;
  onTogglePause: () => void;
  onStop: () => void;
}

/** Small always-on-top panel: state, latency, mute and stop. */
export function CompactPanelView({
  session,
  error,
  onToggleMute,
  onTogglePause,
  onStop,
}: CompactPanelViewProps) {
  const t = useT();
  return (
    <div className="surface" role="region" aria-label={t("compact.region")}>
      <h1>{t("compact.title")}</h1>
      {error ? <ActionErrorBanner {...error} /> : null}
      {isPaused(session) ? <Banner kind="warning">{t("compact.paused")}</Banner> : null}
      {session.banner ? <Banner kind={session.banner.kind}>{session.banner.message}</Banner> : null}
      {session.status === "starting" ? (
        <LoadingState label={t("live.starting")} />
      ) : (
        <p>{t("live.state", { state: t(RUN_STATE_KEY[session.status]) })}</p>
      )}
      {session.latency ? (
        <p className="muted">{t("compact.latency", { p50: session.latency.p50Ms })}</p>
      ) : null}
      <MuteControls session={session} onToggleMute={onToggleMute} onTogglePause={onTogglePause} />
      <Button variant="primary" disabled={session.status !== "running"} onClick={onStop}>
        {t("live.stop")}
      </Button>
    </div>
  );
}
