// @vitest-environment jsdom
import { act, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { sampleModels } from "../../ipc/mockTransport";
import { createManualClock } from "../../shared/testing/manualClock";
import { renderApp, type RenderAppOptions } from "../../shared/testing/renderApp";

const readyModels = () => sampleModels().map((m) => ({ ...m, state: "ok" as const }));
const ready = (options: RenderAppOptions = {}): RenderAppOptions => ({
  models: readyModels(),
  ...options,
});

const PANEL = "Before you start";
const HEADPHONES = "I use headphones";
const MEETING_MIC = "My meeting app microphone is set to Interlo";

function startCalls(invoke: { mock: { calls: unknown[][] } }) {
  return invoke.mock.calls.filter(([cmd]) => cmd === "start_meeting").map(([, args]) => args);
}

async function openPanelOnLive(options: RenderAppOptions = {}) {
  const app = renderApp({ screen: "live", ...options });
  await app.user.click(await screen.findByRole("button", { name: "Start meeting" }));
  const panel = await screen.findByRole("region", { name: PANEL });
  return { ...app, panel };
}

describe("Before you start panel on Live", () => {
  it("opens when Start is pressed, with a meeting app list and two confirmations", async () => {
    const { panel } = await openPanelOnLive(ready());

    const select = within(panel).getByRole("combobox", { name: "Meeting app" });
    const options = within(select)
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).textContent);
    expect(options).toEqual([
      "Choose a meeting app",
      "Meeting app in the browser",
      "Meeting app (desktop)",
    ]);
    expect(
      (within(panel).getByRole("checkbox", { name: HEADPHONES }) as HTMLInputElement).checked,
    ).toBe(false);
    expect(
      (within(panel).getByRole("checkbox", { name: MEETING_MIC }) as HTMLInputElement).checked,
    ).toBe(false);
  });

  it("does not start the meeting until the panel is confirmed", async () => {
    const { transport } = await openPanelOnLive(ready());
    const invoke = vi.spyOn(transport, "invoke");
    expect(startCalls(invoke)).toEqual([]);
    expect(screen.queryByText("Starting meeting…")).toBeNull();
  });

  it("sends the chosen mode, meeting app id and both confirmations", async () => {
    const { user, panel, transport } = await openPanelOnLive(ready());
    const invoke = vi.spyOn(transport, "invoke");

    await user.selectOptions(screen.getByRole("combobox", { name: "Mode" }), "listen");
    await user.selectOptions(
      within(panel).getByRole("combobox", { name: "Meeting app" }),
      "desktop",
    );
    await user.click(within(panel).getByRole("checkbox", { name: HEADPHONES }));
    await user.click(within(panel).getByRole("checkbox", { name: MEETING_MIC }));
    await user.click(within(panel).getByRole("button", { name: "Start now" }));

    await screen.findByText("Starting meeting…");
    expect(startCalls(invoke)).toEqual([
      {
        mode: "listen",
        meetingAppId: "desktop",
        checks: { headphonesConfirmed: true, meetingMicConfirmed: true },
      },
    ]);
  });

  it("keeps the mode switch outside the panel and Cancel closes the panel", async () => {
    const { user, panel } = await openPanelOnLive(ready());
    expect(within(panel).queryByRole("combobox", { name: "Mode" })).toBeNull();
    expect(screen.getByRole("combobox", { name: "Mode" })).toBeTruthy();

    await user.click(within(panel).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("region", { name: PANEL })).toBeNull();
    expect(screen.getByRole("button", { name: "Start meeting" })).toBeTruthy();
  });

  it("START_CHECK_FAILED shows the failed checks as fix steps and keeps the panel and choices", async () => {
    const { user, panel } = await openPanelOnLive(ready());
    await user.click(within(panel).getByRole("checkbox", { name: MEETING_MIC }));
    await user.click(within(panel).getByRole("button", { name: "Start now" }));

    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByText("Confirm before starting: headphones in use, meeting app chosen."),
    ).toBeTruthy();
    const steps = within(alert)
      .getAllByRole("listitem")
      .map((li) => li.textContent);
    expect(steps).toHaveLength(2);
    expect(steps[0]).toContain("Put on headphones");
    expect(steps[1]).toContain("Choose your meeting app");

    // The panel stays open and keeps what the owner already confirmed.
    const again = screen.getByRole("region", { name: PANEL });
    expect(
      (within(again).getByRole("checkbox", { name: MEETING_MIC }) as HTMLInputElement).checked,
    ).toBe(true);
    expect(screen.queryByText("Meeting running.")).toBeNull();
  });

  it("starts once the owner has done the fix steps", async () => {
    const { user, panel } = await openPanelOnLive(ready());
    await user.click(within(panel).getByRole("button", { name: "Start now" }));
    await screen.findByRole("alert");

    const again = screen.getByRole("region", { name: PANEL });
    await user.click(within(again).getByRole("checkbox", { name: HEADPHONES }));
    await user.click(within(again).getByRole("checkbox", { name: MEETING_MIC }));
    await user.selectOptions(
      within(again).getByRole("combobox", { name: "Meeting app" }),
      "browser",
    );
    await user.click(within(again).getByRole("button", { name: "Start now" }));

    await screen.findByText("Starting meeting…");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("SETUP_INCOMPLETE keeps working: the message and Open Setup, and the panel closes", async () => {
    const { user, panel } = await openPanelOnLive();
    await user.click(within(panel).getByRole("button", { name: "Start now" }));

    expect(await screen.findByText("Finish setup first. Not turned on yet: Models.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open Setup" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: PANEL })).toBeNull();
  });
});

describe("Before you start panel on Home", () => {
  it("opens from Home with its own mode choice, and a started meeting opens Live", async () => {
    const { user, transport } = renderApp({ screen: "home", ...ready() });
    const invoke = vi.spyOn(transport, "invoke");
    await user.click(await screen.findByRole("button", { name: "Start meeting" }));

    const panel = await screen.findByRole("region", { name: PANEL });
    await user.selectOptions(within(panel).getByRole("combobox", { name: "Mode" }), "speak");
    await user.selectOptions(
      within(panel).getByRole("combobox", { name: "Meeting app" }),
      "browser",
    );
    await user.click(within(panel).getByRole("checkbox", { name: HEADPHONES }));
    await user.click(within(panel).getByRole("checkbox", { name: MEETING_MIC }));
    await user.click(within(panel).getByRole("button", { name: "Start now" }));

    expect(await screen.findByRole("heading", { name: "Live meeting" })).toBeTruthy();
    expect(startCalls(invoke)).toEqual([
      {
        mode: "speak",
        meetingAppId: "browser",
        checks: { headphonesConfirmed: true, meetingMicConfirmed: true },
      },
    ]);
  });

  it("shows START_CHECK_FAILED inside the panel on Home and stays on Home", async () => {
    const { user } = renderApp({ screen: "home", ...ready() });
    await user.click(await screen.findByRole("button", { name: "Start meeting" }));
    const panel = await screen.findByRole("region", { name: PANEL });
    await user.click(within(panel).getByRole("button", { name: "Start now" }));

    const alert = await screen.findByRole("alert");
    expect(within(alert).getAllByRole("listitem")).toHaveLength(3);
    expect(screen.getByRole("heading", { name: "Home" })).toBeTruthy();
    expect(screen.getByRole("region", { name: PANEL })).toBeTruthy();
  });

  it("has no Start button while setup is not ready", async () => {
    renderApp({ screen: "home" });
    await screen.findByRole("button", { name: "Open Setup" });
    expect(screen.queryByRole("button", { name: "Start meeting" })).toBeNull();
  });
});

describe("10-minute reminder on Live", () => {
  const REMINDER = "Check that your meeting app microphone is still set to Interlo.";
  const TEN_MINUTES = 600_000;

  async function runMeeting() {
    const clock = createManualClock(Date.UTC(2026, 10, 17, 9, 0, 0));
    const app = renderApp({ screen: "live", clock, ...ready() });
    await app.user.click(await screen.findByRole("button", { name: "Start meeting" }));
    const panel = await screen.findByRole("region", { name: PANEL });
    await app.user.selectOptions(
      within(panel).getByRole("combobox", { name: "Meeting app" }),
      "browser",
    );
    await app.user.click(within(panel).getByRole("checkbox", { name: HEADPHONES }));
    await app.user.click(within(panel).getByRole("checkbox", { name: MEETING_MIC }));
    await app.user.click(within(panel).getByRole("button", { name: "Start now" }));
    await screen.findByText("Starting meeting…");
    await app.emit("meeting://status", { state: "running" });
    await screen.findByText("Meeting running.");
    return { ...app, clock };
  }

  it("appears after 10 minutes of running, not one millisecond before", async () => {
    const { clock } = await runMeeting();
    expect(screen.queryByText(REMINDER)).toBeNull();

    act(() => clock.advance(TEN_MINUTES - 1));
    expect(screen.queryByText(REMINDER)).toBeNull();

    act(() => clock.advance(1));
    expect(screen.getByText(REMINDER)).toBeTruthy();
  });

  it("can be dismissed and comes back at the next 10-minute mark", async () => {
    const { user, clock } = await runMeeting();
    act(() => clock.advance(TEN_MINUTES));
    await user.click(screen.getByRole("button", { name: "Dismiss reminder" }));
    expect(screen.queryByText(REMINDER)).toBeNull();

    act(() => clock.advance(TEN_MINUTES - 1));
    expect(screen.queryByText(REMINDER)).toBeNull();
    act(() => clock.advance(1));
    expect(screen.getByText(REMINDER)).toBeTruthy();
  });

  it("goes away when the meeting stops", async () => {
    const { user, clock } = await runMeeting();
    act(() => clock.advance(TEN_MINUTES));
    expect(screen.getByText(REMINDER)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Stop meeting" }));
    await screen.findByRole("button", { name: "Start meeting" });
    expect(screen.queryByText(REMINDER)).toBeNull();
  });
});
