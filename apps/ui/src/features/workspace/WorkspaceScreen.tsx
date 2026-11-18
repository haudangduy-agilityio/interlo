import { useState } from "react";
import { useToast } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { toUiError } from "../../ipc/errors";
import { useT } from "../../shared/i18n";
import { useActionFailure } from "../../shared/useActionFailure";
import { useAsync } from "../../shared/useAsync";
import { WorkspaceView, type WorkspaceTabId } from "./WorkspaceView";

/**
 * Midnight today in the owner's local time zone, in epoch milliseconds. "Today" follows the
 * Mac's clock and zone, not UTC: a note from 23:30 local time still counts as today.
 * `createdAtMs` is an absolute time, so the comparison needs no zone conversion.
 */
export function startOfToday(now: Date = new Date()): number {
  const d = new Date(now);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function WorkspaceScreen() {
  const client = useClient();
  const toast = useToast();
  const t = useT();
  const { state, reload, setData } = useAsync(() => client.listNotes(), [client]);
  const [tab, setTab] = useState<WorkspaceTabId>("today");
  const [draftText, setDraftText] = useState("");
  const [draftTag, setDraftTag] = useState("");
  const [adding, setAdding] = useState(false);
  const [query, setQuery] = useState("");

  const { banner, fail, clear } = useActionFailure();

  const addNote = () => {
    clear();
    setAdding(true);
    client
      .addNote({ text: draftText, tag: draftTag.trim() === "" ? null : draftTag.trim() })
      .then((note) => {
        setData((all) => [note, ...all]);
        setDraftText("");
        setDraftTag("");
        toast(t("workspace.noteAdded"));
      })
      .catch((e: unknown) => {
        toast(toUiError(e).message, "error");
        fail(e);
      })
      .finally(() => setAdding(false));
  };

  return (
    <WorkspaceView
      notes={state}
      tab={tab}
      onTabChange={setTab}
      todayStartMs={startOfToday()}
      draftText={draftText}
      draftTag={draftTag}
      adding={adding}
      error={banner}
      onDraftTextChange={setDraftText}
      onDraftTagChange={setDraftTag}
      onAddNote={addNote}
      query={query}
      onQueryChange={setQuery}
      onRetry={reload}
    />
  );
}
