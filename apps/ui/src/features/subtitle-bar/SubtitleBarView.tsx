import { latestIncomingCaption, type SessionState } from "../../ipc/meetingSession";
import { useT } from "../../shared/i18n";
import { Banner, LoadingState } from "../../shared/ui";

/** Floating bar with the partner's latest caption line. Outgoing lines never show here. */
export function SubtitleBarView({ session }: { session: SessionState }) {
  const t = useT();
  const last = latestIncomingCaption(session.captions);
  return (
    <div className="surface subtitle-bar" aria-label={t("subtitle.region")} role="region">
      {session.banner ? <Banner kind={session.banner.kind}>{session.banner.message}</Banner> : null}
      {session.status === "starting" ? <LoadingState label={t("live.starting")} /> : null}
      {session.status === "idle" ? <p className="muted">{t("subtitle.idle")}</p> : null}
      {session.status === "running" && !last ? (
        <p className="muted">{t("captions.waiting")}</p>
      ) : null}
      {last ? (
        <p className="subtitle-bar__line">
          <span>{last.sourceText}</span>
          {last.targetText !== null ? <strong> {last.targetText}</strong> : null}
        </p>
      ) : null}
    </div>
  );
}
