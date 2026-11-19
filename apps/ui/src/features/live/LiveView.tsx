import type { ReactNode } from "react";
import { CaptionList } from "./CaptionList";
import { RUN_STATE_KEY } from "../../ipc/labels";
import { isPaused, type SessionState } from "../../ipc/meetingSession";
import type { Direction, MeetingMode } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import {
  ActionErrorBanner,
  Banner,
  Button,
  EmptyState,
  ErrorState,
  LoadingState,
  Page,
  Section,
  type ActionErrorBannerProps,
} from "../../shared/ui";
import { MODE_OPTIONS } from "../start";
import { MeetingReminder } from "./MeetingReminder";
import { SessionBanner } from "./SessionNotices";
import { MuteControls } from "./MuteControls";

export interface LiveViewProps {
  session: SessionState;
  mode: MeetingMode;
  /** The last refused mute, pause or stop. */
  error?: ActionErrorBannerProps;
  /** The "Before you start" panel while it is open. Shown under the Start button. */
  panel?: ReactNode;
  onModeChange: (mode: MeetingMode) => void;
  onStart: () => void;
  onStop: () => void;
  onToggleMute: (direction: Direction) => void;
  onTogglePause: () => void;
  onOpenSetup: () => void;
  onDismissBanner: () => void;
}

function StartFailureNotice({
  session,
  onStart,
  onOpenSetup,
}: Pick<LiveViewProps, "session" | "onStart" | "onOpenSetup">) {
  const t = useT();
  const failure = session.startFailure;
  if (!failure) return null;
  // Failed per-meeting checks are shown by the open "Before you start" panel.
  if (failure.kind === "checks-failed") return null;
  if (failure.kind === "setup-incomplete") {
    // The message comes from the command and is shown unchanged.
    return (
      <Banner kind="error" actions={<Button onClick={onOpenSetup}>{t("live.openSetup")}</Button>}>
        {failure.message}
      </Banner>
    );
  }
  return <ErrorState message={failure.message} onRetry={onStart} />;
}

function Controls(props: LiveViewProps) {
  const { session, mode, onModeChange, onStart, onStop, onOpenSetup } = props;
  const t = useT();
  if (session.status === "starting") return <LoadingState label={t("live.starting")} />;
  if (session.status === "stopping") return <LoadingState label={t("live.stopping")} />;
  if (session.status === "running") {
    return (
      <div className="stack">
        <p>{t("live.running")}</p>
        <MuteControls
          session={session}
          onToggleMute={props.onToggleMute}
          onTogglePause={props.onTogglePause}
        />
        <div>
          <Button variant="primary" onClick={onStop}>
            {t("live.stop")}
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="stack">
      <StartFailureNotice session={session} onStart={onStart} onOpenSetup={onOpenSetup} />
      <label className="field">
        <span>{t("live.mode")}</span>
        <select value={mode} onChange={(e) => onModeChange(e.target.value as MeetingMode)}>
          {MODE_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {t(o.labelKey)}
            </option>
          ))}
        </select>
      </label>
      <div>
        <Button variant="primary" onClick={onStart}>
          {t("live.start")}
        </Button>
      </div>
      {props.panel ?? null}
    </div>
  );
}

function LatencyReadout({ session }: { session: SessionState }) {
  const t = useT();
  if (session.status !== "running") return <p className="muted">{t("live.latencyIdle")}</p>;
  if (!session.latency) return <p className="muted">{t("live.latencyWaiting")}</p>;
  return <p>{t("live.latency", { p50: session.latency.p50Ms, p95: session.latency.p95Ms })}</p>;
}

function CaptionsRegion({ session }: { session: SessionState }) {
  const t = useT();
  if (session.captions.length === 0 && session.status !== "running") {
    return (
      <EmptyState title={t("live.captionsEmptyTitle")} description={t("live.captionsEmptyBody")} />
    );
  }
  return <CaptionList captions={session.captions} />;
}

/** Live meeting screen without hooks. The container passes the session and handlers in. */
export function LiveView(props: LiveViewProps) {
  const { session, error, onDismissBanner } = props;
  const t = useT();
  return (
    <Page title={t("nav.live")}>
      {error ? <ActionErrorBanner {...error} /> : null}
      {isPaused(session) ? <Banner kind="warning">{t("live.paused")}</Banner> : null}
      <SessionBanner session={session} onDismiss={onDismissBanner} />
      {session.status === "running" && session.startedAtMs !== null ? (
        <MeetingReminder startedAtMs={session.startedAtMs} />
      ) : null}
      <Section title={t("live.sectionControls")}>
        <Controls {...props} />
      </Section>
      <Section title={t("live.sectionStatus")}>
        <p>{t("live.state", { state: t(RUN_STATE_KEY[session.status]) })}</p>
        <LatencyReadout session={session} />
      </Section>
      <Section title={t("live.sectionCaptions")}>
        <CaptionsRegion session={session} />
      </Section>
    </Page>
  );
}
