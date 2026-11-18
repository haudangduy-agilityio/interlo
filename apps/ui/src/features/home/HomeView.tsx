import type { ReactNode } from "react";
import { surfaceHref, type SurfaceId } from "../../app/screens";
import type { MeetingSummary, SetupStatus } from "../../ipc/types";
import { formatDateTime } from "../../shared/format";
import { useLanguage, useT, type MessageKey } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import { AsyncContent, Banner, Button, EmptyState, Page, Section } from "../../shared/ui";
import { MeetingStatusBadge } from "../meetings";

export interface HomeViewProps {
  setup: Async<SetupStatus>;
  meetings: Async<MeetingSummary[]>;
  onRetrySetup: () => void;
  onRetryMeetings: () => void;
  /** Start was pressed: the screen opens the "Before you start" panel. */
  onStartMeeting: () => void;
  onGoToLive: () => void;
  /** The "Before you start" panel while it is open. */
  startPanel?: ReactNode;
  onOpenSetup: () => void;
  onOpenMeeting: (meetingId: string) => void;
  onSeeAllMeetings: () => void;
}

const PANELS: readonly { id: SurfaceId; labelKey: MessageKey }[] = [
  { id: "subtitle-bar", labelKey: "home.panel.subtitleBar" },
  { id: "compact-panel", labelKey: "home.panel.compactPanel" },
  { id: "quick-actions", labelKey: "home.panel.quickActions" },
];

export function HomeView(props: HomeViewProps) {
  const t = useT();
  const { language } = useLanguage();
  return (
    <Page title={t("nav.home")}>
      <Section title={t("home.readiness")}>
        <AsyncContent
          state={props.setup}
          onRetry={props.onRetrySetup}
          loadingLabel={t("home.checking")}
        >
          {(status) =>
            status.canStart ? (
              <div className="stack">
                <Banner
                  kind="info"
                  actions={
                    <>
                      <Button variant="primary" onClick={props.onStartMeeting}>
                        {t("live.start")}
                      </Button>
                      <Button onClick={props.onGoToLive}>{t("home.goLive")}</Button>
                    </>
                  }
                >
                  {t("home.ready")}
                </Banner>
                {props.startPanel ?? null}
              </div>
            ) : (
              <Banner
                kind="warning"
                actions={<Button onClick={props.onOpenSetup}>{t("live.openSetup")}</Button>}
              >
                {t("home.notOnYet", { groups: status.missingGroups.join(", ") })}
              </Banner>
            )
          }
        </AsyncContent>
      </Section>
      <Section
        title={t("home.recent")}
        actions={
          <Button variant="ghost" onClick={props.onSeeAllMeetings}>
            {t("home.seeAll")}
          </Button>
        }
      >
        <AsyncContent
          state={props.meetings}
          onRetry={props.onRetryMeetings}
          loadingLabel={t("meetings.loading")}
          isEmpty={(all) => all.length === 0}
          empty={
            <EmptyState title={t("home.noMeetingsTitle")} description={t("home.noMeetingsBody")} />
          }
        >
          {(all) => (
            <ul className="list" aria-label={t("home.recent")}>
              {all.slice(0, 3).map((m) => (
                <li key={m.id} className="list__row">
                  <Button variant="ghost" onClick={() => props.onOpenMeeting(m.id)}>
                    {m.title}
                  </Button>
                  <span className="muted">{formatDateTime(m.startedAtMs, language)}</span>
                  <MeetingStatusBadge status={m.status} />
                </li>
              ))}
            </ul>
          )}
        </AsyncContent>
      </Section>
      <Section title={t("home.panels")}>
        <ul className="list" aria-label={t("home.panelsLabel")}>
          {PANELS.map((p) => (
            <li key={p.id} className="list__row">
              <a href={surfaceHref(p.id)}>{t(p.labelKey)}</a>
            </li>
          ))}
        </ul>
      </Section>
    </Page>
  );
}
