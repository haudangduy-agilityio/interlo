import type { Note } from "../../ipc/types";
import { formatDateTime } from "../../shared/format";
import { useLanguage, useT, type MessageKey } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import {
  ActionErrorBanner,
  AsyncContent,
  Button,
  EmptyState,
  NotAvailable,
  Page,
  Section,
  Tabs,
  type ActionErrorBannerProps,
} from "../../shared/ui";

export const WORKSPACE_TABS = [
  { id: "today", labelKey: "workspace.tab.today" },
  { id: "notes", labelKey: "workspace.tab.notes" },
  { id: "tasks", labelKey: "workspace.tab.tasks" },
  { id: "search", labelKey: "workspace.tab.search" },
] as const satisfies readonly { id: string; labelKey: MessageKey }[];
export type WorkspaceTabId = (typeof WORKSPACE_TABS)[number]["id"];

export function notesFromToday(notes: readonly Note[], todayStartMs: number): Note[] {
  return notes.filter((n) => n.createdAtMs >= todayStartMs);
}

export function searchNotes(notes: readonly Note[], query: string): Note[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return [];
  return notes.filter(
    (n) => n.text.toLowerCase().includes(q) || (n.tag ?? "").toLowerCase().includes(q),
  );
}

export interface WorkspaceViewProps {
  notes: Async<Note[]>;
  tab: WorkspaceTabId;
  onTabChange: (tab: WorkspaceTabId) => void;
  /** Start of the local day, in milliseconds. */
  todayStartMs: number;
  draftText: string;
  draftTag: string;
  adding: boolean;
  /** The last refused note. The draft stays in the form. */
  error?: ActionErrorBannerProps;
  onDraftTextChange: (text: string) => void;
  onDraftTagChange: (tag: string) => void;
  onAddNote: () => void;
  query: string;
  onQueryChange: (query: string) => void;
  onRetry: () => void;
}

function NoteList({ notes, label }: { notes: readonly Note[]; label: string }) {
  const { language } = useLanguage();
  return (
    <ul className="list" aria-label={label}>
      {notes.map((n) => (
        <li key={n.id} className="list__row">
          <span>{n.text}</span>
          {n.tag ? <span className="badge">{n.tag}</span> : null}
          <span className="muted">{formatDateTime(n.createdAtMs, language)}</span>
        </li>
      ))}
    </ul>
  );
}

function TodayTab({
  notes,
  todayStartMs,
  onRetry,
}: Pick<WorkspaceViewProps, "notes" | "todayStartMs" | "onRetry">) {
  const t = useT();
  return (
    <Section title={t("workspace.todaysNotes")}>
      <AsyncContent state={notes} onRetry={onRetry} loadingLabel={t("workspace.loadingNotes")}>
        {(all) => {
          const today = notesFromToday(all, todayStartMs);
          return today.length === 0 ? (
            <EmptyState
              title={t("workspace.todayEmptyTitle")}
              description={t("workspace.todayEmptyBody")}
            />
          ) : (
            <NoteList notes={today} label={t("workspace.todayList")} />
          );
        }}
      </AsyncContent>
    </Section>
  );
}

function NotesTab(props: WorkspaceViewProps) {
  const t = useT();
  return (
    <>
      <Section title={t("workspace.newNote")}>
        {props.error ? <ActionErrorBanner {...props.error} /> : null}
        <form
          className="stack"
          onSubmit={(e) => {
            e.preventDefault();
            props.onAddNote();
          }}
        >
          <label className="field">
            <span>{t("workspace.noteLabel")}</span>
            <textarea
              value={props.draftText}
              onChange={(e) => props.onDraftTextChange(e.target.value)}
              rows={3}
            />
          </label>
          <label className="field">
            <span>{t("workspace.tagLabel")}</span>
            <input
              value={props.draftTag}
              onChange={(e) => props.onDraftTagChange(e.target.value)}
            />
          </label>
          <div>
            <Button
              type="submit"
              variant="primary"
              disabled={props.adding || props.draftText.trim().length === 0}
            >
              {props.adding ? t("workspace.adding") : t("workspace.addNote")}
            </Button>
          </div>
        </form>
      </Section>
      <Section title={t("workspace.allNotes")}>
        <AsyncContent
          state={props.notes}
          onRetry={props.onRetry}
          loadingLabel={t("workspace.loadingNotes")}
          isEmpty={(all) => all.length === 0}
          empty={
            <EmptyState
              title={t("workspace.noNotesTitle")}
              description={t("workspace.noNotesBody")}
            />
          }
        >
          {(all) => <NoteList notes={all} label={t("workspace.allList")} />}
        </AsyncContent>
      </Section>
    </>
  );
}

function TasksTab() {
  const t = useT();
  return (
    <Section title={t("workspace.tasks")}>
      <NotAvailable>{t("workspace.tasksBody")}</NotAvailable>
    </Section>
  );
}

function SearchTab({
  notes,
  query,
  onQueryChange,
  onRetry,
}: Pick<WorkspaceViewProps, "notes" | "query" | "onQueryChange" | "onRetry">) {
  const t = useT();
  return (
    <>
      <Section title={t("workspace.searchNotes")}>
        <label className="field">
          <span>{t("workspace.searchLabel")}</span>
          <input type="search" value={query} onChange={(e) => onQueryChange(e.target.value)} />
        </label>
      </Section>
      <Section title={t("workspace.results")}>
        <AsyncContent state={notes} onRetry={onRetry} loadingLabel={t("workspace.loadingNotes")}>
          {(all) => {
            if (query.trim().length === 0)
              return (
                <EmptyState
                  title={t("workspace.typeToSearch")}
                  description={t("workspace.searchHint")}
                />
              );
            const found = searchNotes(all, query);
            return found.length === 0 ? (
              <EmptyState
                title={t("workspace.noMatch")}
                action={
                  <Button onClick={() => onQueryChange("")}>{t("common.clearSearch")}</Button>
                }
              />
            ) : (
              <NoteList notes={found} label={t("workspace.searchResults")} />
            );
          }}
        </AsyncContent>
      </Section>
    </>
  );
}

function TabBody(props: WorkspaceViewProps) {
  switch (props.tab) {
    case "today":
      return (
        <TodayTab notes={props.notes} todayStartMs={props.todayStartMs} onRetry={props.onRetry} />
      );
    case "notes":
      return <NotesTab {...props} />;
    case "tasks":
      return <TasksTab />;
    case "search":
      return (
        <SearchTab
          notes={props.notes}
          query={props.query}
          onQueryChange={props.onQueryChange}
          onRetry={props.onRetry}
        />
      );
  }
}

export function WorkspaceView(props: WorkspaceViewProps) {
  const t = useT();
  return (
    <Page title={t("nav.workspace")}>
      <Tabs
        label={t("workspace.sections")}
        tabs={WORKSPACE_TABS.map((tab) => ({ id: tab.id, label: t(tab.labelKey) }))}
        active={props.tab}
        onChange={props.onTabChange}
      />
      <div
        role="tabpanel"
        id={`panel-${props.tab}`}
        aria-labelledby={`tab-${props.tab}`}
        className="stack"
      >
        <TabBody {...props} />
      </div>
    </Page>
  );
}
