import { surfaceHref, type SurfaceId } from "../../app/screens";
import type { MeetingSummary, Note } from "../../ipc/types";
import { formatDuration } from "../../shared/format";
import { localeFor, useLanguage, useT, type MessageKey } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import {
  AsyncContent,
  Button,
  Chip,
  EmptyState,
  Icon,
  KeyCap,
  NotAvailable,
  Page,
  Section,
} from "../../shared/ui";
import { QUICK_ACTIONS } from "./quickActions";
import { meetingsOfToday, notesOfToday } from "./today";

export interface HomeViewProps {
  /** The time the screen shows "today" for. */
  nowMs: number;
  meetings: Async<MeetingSummary[]>;
  notes: Async<Note[]>;
  onRetryMeetings: () => void;
  onRetryNotes: () => void;
  onStartMeeting: () => void;
  onOpenMeeting: (meetingId: string) => void;
  onSeeAllMeetings: () => void;
  onOpenWorkspace: () => void;
}

// The compact panel opens from the top bar; the other two floating panels stay listed here
// until the real windows exist (SPEC O-1).
const PANELS: readonly { id: SurfaceId; labelKey: MessageKey }[] = [
  { id: "subtitle-bar", labelKey: "home.panel.subtitleBar" },
  { id: "quick-actions", labelKey: "home.panel.quickActions" },
];

function timeOfDay(ms: number, locale: string): string {
  return new Date(ms).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
}

/** Home is the Today view of the demo: quick actions, tasks, today's notes and meetings. */
export function HomeView(props: HomeViewProps) {
  const t = useT();
  const { language } = useLanguage();
  const locale = localeFor(language);
  const date = new Date(props.nowMs).toLocaleDateString(locale, {
    weekday: "long",
    day: "numeric",
    month: "long",
  });
  return (
    <Page title={t("home.title")} lead={t("home.lead", { date })}>
      <div className="actions-row">
        {QUICK_ACTIONS.map((a) => (
          <Button key={a.id} unavailable>
            {t(a.labelKey)}
            <KeyCap>{a.keys}</KeyCap>
          </Button>
        ))}
      </div>
      <div className="grid grid--2">
        <Section title={t("home.tasks")}>
          <NotAvailable>{t("workspace.tasksBody")}</NotAvailable>
        </Section>
        <Section
          title={t("home.todaysNotes")}
          actions={
            <Button size="sm" onClick={props.onOpenWorkspace}>
              {t("home.openWorkspace")}
            </Button>
          }
        >
          <AsyncContent
            state={props.notes}
            onRetry={props.onRetryNotes}
            loadingLabel={t("workspace.loadingNotes")}
            isEmpty={(all) => notesOfToday(all, props.nowMs).length === 0}
            empty={
              <EmptyState
                title={t("home.notesEmptyTitle")}
                description={t("home.notesEmptyBody")}
              />
            }
          >
            {(all) => (
              <div role="list" aria-label={t("home.todaysNotes")}>
                {notesOfToday(all, props.nowMs).map((n) => (
                  <div key={n.id} role="listitem" className="rowi">
                    <div>
                      <div className="rowi__desc mono">{timeOfDay(n.createdAtMs, locale)}</div>
                      <div>{n.text}</div>
                    </div>
                    {n.tag ? <Chip>{n.tag}</Chip> : null}
                  </div>
                ))}
              </div>
            )}
          </AsyncContent>
        </Section>
        <Section
          title={t("home.todaysMeetings")}
          actions={
            <Button size="sm" variant="ghost" onClick={props.onSeeAllMeetings}>
              {t("home.seeAll")}
            </Button>
          }
        >
          <AsyncContent
            state={props.meetings}
            onRetry={props.onRetryMeetings}
            loadingLabel={t("meetings.loading")}
            isEmpty={(all) => meetingsOfToday(all, props.nowMs).length === 0}
            empty={
              <EmptyState
                title={t("home.meetingsEmptyTitle")}
                description={t("home.meetingsEmptyBody")}
              />
            }
          >
            {(all) => (
              <div role="list" aria-label={t("home.todaysMeetings")}>
                {meetingsOfToday(all, props.nowMs).map((m) => (
                  <div key={m.id} role="listitem" className="rowi">
                    <div>
                      <div className="rowi__title">{m.title}</div>
                      <div className="rowi__desc">
                        {timeOfDay(m.startedAtMs, locale)} · {formatDuration(m.durationSec)}
                      </div>
                    </div>
                    <Button
                      size="sm"
                      aria-label={t("home.openMeeting", { title: m.title })}
                      onClick={() => props.onOpenMeeting(m.id)}
                    >
                      {t("home.open")}
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </AsyncContent>
          <div>
            <Button variant="primary" onClick={props.onStartMeeting}>
              <Icon name="play" size={14} />
              {t("live.start")}
            </Button>
          </div>
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
      </div>
    </Page>
  );
}
