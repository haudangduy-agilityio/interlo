import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { sampleModels, sampleNotes } from "../ipc/mockTransport";
import { initialSession } from "../ipc/meetingSession";
import type { SetupStatus } from "../ipc/types";
import { CompactPanelView } from "./compact-panel";
import { ModelsTab } from "./settings";
import { QuickActionsPanel } from "./quick-actions";
import { SetupView } from "./setup";
import { SubtitleBarView } from "./subtitle-bar";
import { WorkspaceView, searchNotes } from "./workspace";

const noop = () => {};

const incomplete: SetupStatus = {
  canStart: false,
  missingGroups: ["Models"],
  items: [
    { id: "mic", group: "devices", state: "ok", detail: "Default microphone found." },
    { id: "enV", group: "models", state: "missing", detail: "English voice model is not ready." },
  ],
};

describe("SetupView", () => {
  it("lists items by group and offers a download for a missing model", () => {
    const html = renderToStaticMarkup(
      <SetupView
        status={{ status: "done", data: incomplete }}
        onRecheck={noop}
        onDownload={noop}
      />,
    );
    expect(html).toContain("Not turned on yet: Models.");
    expect(html).toContain('aria-label="Download English voice model"');
    expect(html).not.toContain('aria-label="Download Microphone"');
  });

  it("shows loading and error states", () => {
    expect(
      renderToStaticMarkup(
        <SetupView status={{ status: "loading" }} onRecheck={noop} onDownload={noop} />,
      ),
    ).toContain('role="status"');
    expect(
      renderToStaticMarkup(
        <SetupView
          status={{ status: "error", error: { code: "UNKNOWN", message: "Setup check failed." } }}
          onRecheck={noop}
          onDownload={noop}
        />,
      ),
    ).toContain("Setup check failed.");
  });
});

describe("ModelsTab", () => {
  it("shows a download button only for models that are not ready", () => {
    const html = renderToStaticMarkup(
      <ModelsTab
        models={{ status: "done", data: sampleModels() }}
        onDownload={noop}
        onRetry={noop}
      />,
    );
    expect(html).toContain('aria-label="Download English voice"');
    expect(html).not.toContain('aria-label="Download Translation"');
  });

  it("shows an empty state for no models", () => {
    expect(
      renderToStaticMarkup(
        <ModelsTab models={{ status: "done", data: [] }} onDownload={noop} onRetry={noop} />,
      ),
    ).toContain("No models listed");
  });
});

describe("WorkspaceView", () => {
  const props = {
    notes: { status: "done", data: sampleNotes() } as const,
    todayStartMs: 0,
    draftText: "",
    draftTag: "",
    adding: false,
    onTabChange: noop,
    onDraftTextChange: noop,
    onDraftTagChange: noop,
    onAddNote: noop,
    query: "",
    onQueryChange: noop,
    onRetry: noop,
  };

  it("has four tabs", () => {
    const html = renderToStaticMarkup(<WorkspaceView {...props} tab="today" />);
    for (const label of ["Today", "Notes", "Tasks", "Search"])
      expect(html).toContain(`>${label}</button>`);
  });

  it("notes tab: empty list shows 'No notes yet' and Add note is disabled without text", () => {
    const html = renderToStaticMarkup(
      <WorkspaceView {...props} notes={{ status: "done", data: [] }} tab="notes" />,
    );
    expect(html).toContain("No notes yet");
    expect(html).toMatch(/<button[^>]*disabled[^>]*>Add note<\/button>/);
  });

  it("search tab: prompts first, then reports no match with Clear search", () => {
    expect(renderToStaticMarkup(<WorkspaceView {...props} tab="search" />)).toContain(
      "Type to search",
    );
    const miss = renderToStaticMarkup(<WorkspaceView {...props} tab="search" query="zzz" />);
    expect(miss).toContain("No notes match");
    expect(miss).toContain(">Clear search</button>");
  });

  it("searchNotes matches text and tag", () => {
    expect(searchNotes(sampleNotes(), "rollout")).toHaveLength(1);
    expect(searchNotes(sampleNotes(), "follow-up")).toHaveLength(1);
    expect(searchNotes(sampleNotes(), "")).toHaveLength(0);
  });
});

describe("QuickActionsPanel", () => {
  it("has the three actions, disabled until there is text", () => {
    const html = renderToStaticMarkup(
      <QuickActionsPanel text="" onTextChange={noop} preview={{ status: "idle" }} onRun={noop} />,
    );
    for (const label of ["Translate", "Fix grammar", "Save note"])
      expect(html).toMatch(new RegExp(`<button[^>]*disabled[^>]*>${label}</button>`));
  });

  it("shows working, done and error previews", () => {
    const render = (preview: Parameters<typeof QuickActionsPanel>[0]["preview"]) =>
      renderToStaticMarkup(
        <QuickActionsPanel text="Hello" onTextChange={noop} preview={preview} onRun={noop} />,
      );
    expect(render({ status: "working", action: "save-note" })).toContain('role="status"');
    expect(render({ status: "done", action: "save-note", message: "Saved as a note." })).toContain(
      "Saved as a note.",
    );
    expect(
      render({ status: "error", action: "translate", message: "Translate is not connected yet." }),
    ).toContain(">Retry</button>");
  });
});

describe("floating surfaces", () => {
  it("subtitle bar covers idle, waiting and a caption", () => {
    const render = (session = initialSession) =>
      renderToStaticMarkup(<SubtitleBarView session={session} />);
    expect(render()).toContain("No meeting is running.");
    expect(render({ ...initialSession, status: "running" })).toContain("Waiting for speech.");
    const withCaption = {
      ...initialSession,
      status: "running" as const,
      captions: [
        {
          segmentId: "l1",
          sourceId: "incoming",
          startMs: 0,
          sourceText: "Hello",
          sourceStatus: "final" as const,
          targetText: "Xin chào",
          targetLanguage: null,
          translationStatus: "final" as const,
        },
      ],
    };
    const html = render(withCaption);
    expect(html).toContain("Hello");
    expect(html).toContain("Xin chào");
  });

  it("subtitle bar shows the partner's latest line even when an outgoing line came last", () => {
    const line = (segmentId: string, sourceId: string, text: string, target: string) => ({
      segmentId,
      sourceId,
      startMs: 0,
      sourceText: text,
      sourceStatus: "final" as const,
      targetText: target,
      targetLanguage: null,
      translationStatus: "final" as const,
    });
    const running = { ...initialSession, status: "running" as const };
    const html = renderToStaticMarkup(
      <SubtitleBarView
        session={{
          ...running,
          captions: [
            line("l1", "incoming", "Partner line", "Câu của đối tác"),
            line("l2", "outgoing", "My own line", "Câu của tôi"),
          ],
        }}
      />,
    );
    expect(html).toContain("Partner line");
    expect(html).not.toContain("My own line");

    const onlyOutgoing = renderToStaticMarkup(
      <SubtitleBarView
        session={{ ...running, captions: [line("l2", "outgoing", "My own line", "Câu của tôi")] }}
      />,
    );
    expect(onlyOutgoing).not.toContain("My own line");
    expect(onlyOutgoing).toContain("Waiting for speech.");
  });

  it("compact panel shows the paused banner and a Stop button that is off when idle", () => {
    const paused = { ...initialSession, status: "running" as const, mutedOut: true, mutedIn: true };
    const html = renderToStaticMarkup(
      <CompactPanelView session={paused} onToggleMute={noop} onTogglePause={noop} onStop={noop} />,
    );
    expect(html).toContain("Paused.");
    const idle = renderToStaticMarkup(
      <CompactPanelView
        session={initialSession}
        onToggleMute={noop}
        onTogglePause={noop}
        onStop={noop}
      />,
    );
    expect(idle).toMatch(/<button[^>]*disabled[^>]*>Stop meeting<\/button>/);
  });
});
