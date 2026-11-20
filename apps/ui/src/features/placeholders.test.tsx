// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { App } from "../App";
import { sampleMeetings } from "../ipc/mockTransport";
import type { SetupStatus } from "../ipc/types";
import { renderApp } from "../shared/testing/renderApp";
import { MeetingDetailView } from "./meeting-detail";
import { HotkeysTab, VocabularyTab, VoicesTab } from "./settings";
import { SetupView } from "./setup";
import { WorkspaceView } from "./workspace";

afterEach(() => {
  cleanup();
  window.location.hash = "";
});

const noop = () => {};

describe("no placeholder text for developers on screen", () => {
  it("settings tabs say 'Not available yet' and what will be there", () => {
    for (const tab of [<VoicesTab key="v" />, <VocabularyTab key="c" />, <HotkeysTab key="h" />]) {
      const html = renderToStaticMarkup(tab);
      expect(html).not.toContain("TODO");
      expect(html).toContain("Not available yet");
    }
    expect(renderToStaticMarkup(<VoicesTab />)).toContain("Voice choices will be here.");
    expect(renderToStaticMarkup(<VocabularyTab />)).toContain(
      "Your recognition terms will be here.",
    );
    expect(renderToStaticMarkup(<HotkeysTab />)).toContain("Shortcut settings will be here.");
  });

  it("meeting detail: key points, action items and transcript say 'Not available yet'", () => {
    const meeting = { ...sampleMeetings()[0], status: "ready" as const };
    const html = renderToStaticMarkup(
      <MeetingDetailView
        meeting={{ status: "done", data: meeting }}
        onBack={noop}
        onRetry={noop}
      />,
    );
    expect(html).not.toContain("TODO");
    expect(html).toContain("The key points of the meeting will be here.");
    expect(html).toContain("The action items of the meeting will be here.");
    expect(html).toContain("The meeting transcript will be here.");
    expect(html.match(/Not available yet/g)?.length).toBeGreaterThanOrEqual(3);
  });

  it("workspace tasks tab says 'Not available yet'", () => {
    const html = renderToStaticMarkup(
      <WorkspaceView
        notes={{ status: "done", data: [] }}
        tab="tasks"
        todayStartMs={0}
        draftText=""
        draftTag=""
        adding={false}
        onTabChange={noop}
        onDraftTextChange={noop}
        onDraftTagChange={noop}
        onAddNote={noop}
        query=""
        onQueryChange={noop}
        onRetry={noop}
      />,
    );
    expect(html).not.toContain("TODO");
    expect(html).toContain("Not available yet");
    expect(html).toContain("Your tasks will be here.");
  });

  it("quick actions: Translate and Fix grammar say 'Not available yet' without a Retry button", async () => {
    const { user } = renderApp({ surface: "quick-actions" });
    await user.type(screen.getByRole("textbox", { name: "Text to work on" }), "Hello there");
    for (const action of ["Translate", "Fix grammar"]) {
      await user.click(screen.getByRole("button", { name: action }));
      const preview = screen.getByRole("region", { name: "Preview" });
      expect(within(preview).getByText("Not available yet")).toBeTruthy();
      expect(preview.textContent).not.toContain("TODO");
      expect(within(preview).queryByRole("button", { name: "Retry" })).toBeNull();
    }
  });
});

describe("failed summary", () => {
  it("shows Retry as disabled with the reason, because no command can retry", () => {
    const meeting = { ...sampleMeetings()[0], status: "failed" as const };
    render(
      <MeetingDetailView
        meeting={{ status: "done", data: meeting }}
        onBack={noop}
        onRetry={noop}
      />,
    );
    expect(screen.getByText("The summary could not be generated.")).toBeTruthy();
    const retry = screen.getByRole("button", { name: "Retry" }) as HTMLButtonElement;
    expect(retry.disabled).toBe(true);
    expect(screen.getByText("Retry is not available yet.")).toBeTruthy();
  });
});

describe("Setup: missing virtual microphone", () => {
  const status = (state: "missing" | "ok"): SetupStatus => ({
    canStart: state === "ok",
    missingGroups: state === "ok" ? [] : ["Virtual microphone"],
    items: [{ id: "vmic", group: "virtualMicrophone", state, detail: "Not installed." }],
  });

  it("offers 'Show setup steps' that expands plain text steps, and hides them again", async () => {
    const user = userEvent.setup();
    render(
      <SetupView
        status={{ status: "done", data: status("missing") }}
        onRecheck={noop}
        onDownload={noop}
      />,
    );
    expect(screen.queryByRole("list", { name: "Setup steps" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Show setup steps" }));
    const steps = screen.getByRole("list", { name: "Setup steps" });
    expect(within(steps).getAllByRole("listitem").length).toBeGreaterThanOrEqual(3);
    expect(steps.textContent).toContain("Check again");

    await user.click(screen.getByRole("button", { name: "Hide setup steps" }));
    expect(screen.queryByRole("list", { name: "Setup steps" })).toBeNull();
  });

  it("shows no setup-steps button when the virtual microphone is ready", () => {
    render(
      <SetupView
        status={{ status: "done", data: status("ok") }}
        onRecheck={noop}
        onDownload={noop}
      />,
    );
    expect(screen.queryByRole("button", { name: /setup steps/ })).toBeNull();
  });
});

describe("brand and floating panels", () => {
  it("the sidebar brand reads Interlo", async () => {
    renderApp();
    const nav = await screen.findByRole("navigation", { name: "Main" });
    const side = nav.closest("aside") as HTMLElement;
    expect(within(side).getByText("Interlo")).toBeTruthy();
    expect(side.textContent).not.toContain("Meeting Interpreter");
  });

  it("a floating panel has a link back to the app", async () => {
    window.location.hash = "#surface=subtitle-bar";
    const user = userEvent.setup();
    render(<App />);
    expect(await screen.findByRole("region", { name: "Subtitle bar" })).toBeTruthy();
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();

    await user.click(screen.getByRole("link", { name: "Back to app" }));
    expect(await screen.findByRole("navigation", { name: "Main" })).toBeTruthy();
  });
});
