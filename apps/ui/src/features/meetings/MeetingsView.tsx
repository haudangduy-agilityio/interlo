import { formatDateTime, formatDuration } from "../../shared/format";
import { useLanguage, useT } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import type { MeetingSummary } from "../../ipc/types";
import { AsyncContent, Button, EmptyState, Page, Section } from "../../shared/ui";
import { MeetingStatusBadge } from "./MeetingStatusBadge";

export function filterMeetings(
  meetings: readonly MeetingSummary[],
  query: string,
): MeetingSummary[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return [...meetings];
  return meetings.filter((m) => m.title.toLowerCase().includes(q));
}

export interface MeetingsViewProps {
  meetings: Async<MeetingSummary[]>;
  query: string;
  onQueryChange: (query: string) => void;
  onClearSearch: () => void;
  onOpen: (meetingId: string) => void;
  onRetry: () => void;
}

function MeetingList({
  items,
  onOpen,
}: {
  items: readonly MeetingSummary[];
  onOpen: (id: string) => void;
}) {
  const t = useT();
  const { language } = useLanguage();
  return (
    <ul className="list" aria-label={t("meetings.listLabel")}>
      {items.map((m) => (
        <li key={m.id} className="list__row">
          <Button variant="ghost" onClick={() => onOpen(m.id)}>
            {m.title}
          </Button>
          <span className="muted">
            {formatDateTime(m.startedAtMs, language)} · {formatDuration(m.durationSec)}
          </span>
          <MeetingStatusBadge status={m.status} />
        </li>
      ))}
    </ul>
  );
}

/** Two different empty states: an empty library, and a search with no match. */
export function MeetingsView({
  meetings,
  query,
  onQueryChange,
  onClearSearch,
  onOpen,
  onRetry,
}: MeetingsViewProps) {
  const t = useT();
  return (
    <Page title={t("nav.meetings")}>
      <Section title={t("meetings.search")}>
        <label className="field">
          <span>{t("meetings.searchLabel")}</span>
          <input type="search" value={query} onChange={(e) => onQueryChange(e.target.value)} />
        </label>
      </Section>
      <Section title={t("meetings.library")}>
        <AsyncContent
          state={meetings}
          onRetry={onRetry}
          loadingLabel={t("meetings.loading")}
          isEmpty={(all) => all.length === 0}
          empty={
            <EmptyState title={t("home.noMeetingsTitle")} description={t("meetings.emptyBody")} />
          }
        >
          {(all) => {
            const shown = filterMeetings(all, query);
            if (shown.length === 0) {
              return (
                <EmptyState
                  title={t("meetings.noMatchTitle")}
                  description={t("meetings.noMatchBody")}
                  action={<Button onClick={onClearSearch}>{t("common.clearSearch")}</Button>}
                />
              );
            }
            return <MeetingList items={shown} onOpen={onOpen} />;
          }}
        </AsyncContent>
      </Section>
    </Page>
  );
}
