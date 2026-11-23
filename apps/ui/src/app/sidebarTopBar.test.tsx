// @vitest-environment jsdom
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { sampleModels } from "../ipc/mockTransport";
import { renderApp, type RenderAppOptions } from "../shared/testing/renderApp";

// M1-51 shell: sidebar (AC-12 to AC-14, AC-32), top bar (AC-15 to AC-19, AC-35, AC-36).
const ready = (options: RenderAppOptions = {}): RenderAppOptions => ({
  models: sampleModels().map((m) => ({ ...m, state: "ok" as const })),
  ...options,
});

const bar = () => within(screen.getByRole("toolbar", { name: "Meeting controls" }));
const mainNav = () => screen.getByRole("navigation", { name: "Main" });
const sidebar = () => mainNav().closest("aside") as HTMLElement;

type User = ReturnType<typeof renderApp>["user"];

async function fillStartDialog(user: User) {
  const dialog = await screen.findByRole("dialog", { name: "Before you start" });
  await user.selectOptions(
    within(dialog).getByRole("combobox", { name: "Meeting app" }),
    "browser",
  );
  await user.click(within(dialog).getByRole("checkbox", { name: "I use headphones" }));
  await user.click(
    within(dialog).getByRole("checkbox", { name: "My meeting app microphone is set to Interlo" }),
  );
  await user.click(within(dialog).getByRole("button", { name: "Start now" }));
}

describe("sidebar (AC-12, AC-32)", () => {
  it("has the brand, a disabled search, five nav entries with icons, the Local-only card and Settings", async () => {
    renderApp(ready());
    await screen.findByText("Everything is ready on this Mac.");
    const aside = sidebar();
    expect(aside.className).toContain("sb");
    expect(aside.querySelector('img[src="/interlo-icon.svg"]')).toBeTruthy();
    expect(within(aside).getByText("Interlo")).toBeTruthy();
    const search = within(aside).getByRole("button", { name: /Search or run a command/ });
    expect((search as HTMLButtonElement).disabled).toBe(true);
    expect(search.textContent).toContain("⌘K");
    const nav = within(mainNav());
    for (const label of ["Today", "Live meeting", "Meetings", "Workspace", "Setup"]) {
      expect(nav.getByRole("button", { name: label }).querySelector("svg")).toBeTruthy();
    }
    expect(within(aside).getByRole("button", { name: "Settings" })).toBeTruthy();
    expect(within(aside).getByText("Local only")).toBeTruthy();
  });

  it("marks the open screen and moves the mark when another entry is pressed", async () => {
    const { user } = renderApp(ready());
    const nav = within(mainNav());
    expect(nav.getByRole("button", { name: "Today" }).getAttribute("aria-current")).toBe("page");
    await user.click(nav.getByRole("button", { name: "Meetings" }));
    expect(nav.getByRole("button", { name: "Meetings" }).getAttribute("aria-current")).toBe("page");
    expect(nav.getByRole("button", { name: "Today" }).hasAttribute("aria-current")).toBe(false);
  });

  it("keeps Meetings marked while a meeting is open", async () => {
    const { user } = renderApp(ready({ screen: "meetings" }));
    await user.click(await screen.findByRole("button", { name: "Weekly planning" }));
    expect(
      within(mainNav()).getByRole("button", { name: "Meetings" }).getAttribute("aria-current"),
    ).toBe("page");
  });

  it("opens the settings sheet", async () => {
    const { user } = renderApp(ready());
    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("dialog")).toBeTruthy();
  });
});

describe("Setup badge and Local-only card (AC-13, AC-14)", () => {
  it("shows how many setup items are not ready, from the setup check", async () => {
    renderApp();
    await screen.findByText("Some setup items are missing.");
    const setup = within(mainNav()).getByRole("button", { name: "Setup" });
    const hint = setup.getAttribute("aria-describedby");
    expect(hint).toBeTruthy();
    expect(document.getElementById(hint ?? "")?.textContent).toMatch(/^Setup items missing: \d+$/);
    expect(setup.querySelector(".chip")?.textContent).toMatch(/^\d+$/);
  });

  it("shows no badge when every item is ready", async () => {
    renderApp(ready());
    await screen.findByText("Everything is ready on this Mac.");
    const setup = within(mainNav()).getByRole("button", { name: "Setup" });
    expect(setup.querySelector(".chip")).toBeNull();
    expect(setup.hasAttribute("aria-describedby")).toBe(false);
  });

  it("shows no badge and says so when the setup check failed", async () => {
    renderApp({ failing: { check_setup: "shell crashed" } });
    await screen.findByText("Could not check setup");
    expect(
      within(mainNav()).getByRole("button", { name: "Setup" }).querySelector(".chip"),
    ).toBeNull();
  });

  it("has a memory meter that is not available and shows no figure", async () => {
    renderApp(ready());
    const meter = await screen.findByRole("meter", { name: "Memory" });
    expect(meter.getAttribute("aria-disabled")).toBe("true");
    expect(meter.hasAttribute("aria-valuenow")).toBe(false);
  });

  it("says Meeting running with the live dot while a meeting runs", async () => {
    const { user, emit } = renderApp(ready());
    await user.click(await bar().findByRole("button", { name: "Start meeting" }));
    await fillStartDialog(user);
    await emit("meeting://status", { state: "running" });
    expect(await within(sidebar()).findByText("Meeting running")).toBeTruthy();
    expect(sidebar().querySelector(".dot--live")).toBeTruthy();
  });
});

describe("top bar, idle (AC-15)", () => {
  it("shows the language pair, a disabled swap, Start and the compact window link", async () => {
    renderApp(ready());
    const inBar = bar();
    expect(inBar.getByText("SPEAKS")).toBeTruthy();
    expect(inBar.getByText("HEARS")).toBeTruthy();
    expect(inBar.getByText("Vietnamese")).toBeTruthy();
    expect(inBar.getByText("English")).toBeTruthy();
    const swap = inBar.getByRole("button", { name: "Swap languages" }) as HTMLButtonElement;
    expect(swap.disabled).toBe(true);
    const pair = inBar.getByRole("button", { name: /^Language pair/ }) as HTMLButtonElement;
    expect(pair.disabled).toBe(true);
    expect(inBar.getByRole("button", { name: "Start meeting" })).toBeTruthy();
    expect(inBar.getByRole("link", { name: "Compact window" }).getAttribute("href")).toBe(
      "#surface=compact-panel",
    );
  });
});

describe("Start from the top bar (AC-16, AC-17, AC-18, AC-35)", () => {
  it("opens the dialog on any screen, returns focus on Cancel, and confirms into Live", async () => {
    const { user } = renderApp(ready({ screen: "meetings" }));
    const start = await bar().findByRole("button", { name: "Start meeting" });
    await user.click(start);
    const dialog = await screen.findByRole("dialog", { name: "Before you start" });
    const mode = within(dialog).getByRole("combobox", { name: "Mode" }) as HTMLSelectElement;
    expect(mode.value).toBe("both");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(start);

    await user.click(start);
    await fillStartDialog(user);
    expect(await screen.findByRole("heading", { name: "Live meeting", level: 1 })).toBeTruthy();
  });

  it("lists the start modes in the menu and opens the dialog with the chosen mode", async () => {
    const { user } = renderApp(ready());
    await user.click(await bar().findByRole("button", { name: "More start options" }));
    const items = screen.getAllByRole("menuitem").map((i) => i.textContent);
    expect(items).toEqual(["Both directionsdefault", "Listen only", "Speak only", "Review setup"]);
    await user.click(screen.getByRole("menuitem", { name: "Listen only" }));
    const dialog = await screen.findByRole("dialog", { name: "Before you start" });
    const mode = within(dialog).getByRole("combobox", { name: "Mode" }) as HTMLSelectElement;
    expect(mode.value).toBe("listen");
  });

  it("moves with the arrow keys and closes on Escape, returning focus to the menu button", async () => {
    const { user } = renderApp(ready());
    const toggle = await bar().findByRole("button", { name: "More start options" });
    await user.click(toggle);
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: /Both directions/ }));
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Listen only" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(toggle);
  });

  it("opens Setup from Review setup", async () => {
    const { user } = renderApp(ready());
    await user.click(await bar().findByRole("button", { name: "More start options" }));
    await user.click(screen.getByRole("menuitem", { name: "Review setup" }));
    expect(await screen.findByRole("heading", { name: "Setup", level: 1 })).toBeTruthy();
  });

  it("opens Setup with a toast naming what is not on, and no dialog, when setup is not ready", async () => {
    const { user } = renderApp({ devices: { vmic: "missing" } });
    await screen.findByText("Some setup items are missing.");
    await user.click(bar().getByRole("button", { name: "Start meeting" }));
    expect(await screen.findByRole("heading", { name: "Setup", level: 1 })).toBeTruthy();
    expect(screen.getByText(/^Finish setup first\. Not turned on yet: .+\.$/)).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    const start = bar().getByRole("button", { name: "Start meeting" }) as HTMLButtonElement;
    expect(start.disabled).toBe(false);
  });

  it("checks setup again when the first check failed, then opens the dialog", async () => {
    const { user, transport } = renderApp(ready({ failing: { check_setup: "shell crashed" } }));
    await screen.findByText("Could not check setup");
    transport.heal("check_setup");
    await user.click(bar().getByRole("button", { name: "Start meeting" }));
    expect(await screen.findByRole("dialog", { name: "Before you start" })).toBeTruthy();
  });

  it("shows an error toast and opens nothing when the check fails again", async () => {
    const { user } = renderApp(ready({ failing: { check_setup: "shell crashed" } }));
    await screen.findByText("Could not check setup");
    await user.click(bar().getByRole("button", { name: "Start meeting" }));
    expect(await screen.findByText(/^Could not check setup: /)).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("top bar, meeting running (AC-19, AC-36)", () => {
  async function runMeeting(options: RenderAppOptions = {}) {
    const app = renderApp(ready(options));
    await app.user.click(await bar().findByRole("button", { name: "Start meeting" }));
    await fillStartDialog(app.user);
    await app.emit("meeting://status", { state: "running" });
    await within(sidebar()).findByText("Meeting running");
    return app;
  }

  it("shows the Live chip without a time and a Stop button instead of Start", async () => {
    await runMeeting();
    expect(bar().getByText("Live").textContent).not.toMatch(/\d/);
    expect(bar().queryByRole("button", { name: "Start meeting" })).toBeNull();
    expect(bar().getByRole("button", { name: "Stop" })).toBeTruthy();
  });

  it("returns to the idle top bar after Stop", async () => {
    const { user } = await runMeeting();
    await user.click(bar().getByRole("button", { name: "Stop" }));
    expect(await bar().findByRole("button", { name: "Start meeting" })).toBeTruthy();
  });

  it("keeps the Live chip and shows an error banner with Retry when stop fails", async () => {
    const { user, transport } = await runMeeting();
    transport.failCommand("stop_meeting", "shell crashed");
    await user.click(bar().getByRole("button", { name: "Stop" }));
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByRole("button", { name: "Retry" })).toBeTruthy();
    expect(bar().getByText("Live")).toBeTruthy();
    const stop = bar().getByRole("button", { name: "Stop" }) as HTMLButtonElement;
    expect(stop.disabled).toBe(false);
    transport.heal("stop_meeting");
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await bar().findByRole("button", { name: "Start meeting" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("goes back to idle with no banner when the shell says there is no meeting", async () => {
    const { user, transport } = await runMeeting();
    transport.failCommand("stop_meeting", {
      code: "NO_MEETING",
      message: "No meeting is running.",
    });
    await user.click(bar().getByRole("button", { name: "Stop" }));
    expect(await bar().findByRole("button", { name: "Start meeting" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("Setup badge follows the Setup screen (AC-13)", () => {
  it("drops the badge once the missing model is downloaded on Setup", async () => {
    const { user } = renderApp({ screen: "setup" });
    const setup = () => within(mainNav()).getByRole("button", { name: "Setup" });
    await screen.findByText("Some setup items are missing.");
    expect(setup().querySelector(".chip")).toBeTruthy();
    await user.click(await screen.findByRole("button", { name: /^Download / }));
    const dialog = await screen.findByRole("dialog", { name: /^Download / });
    await user.click(within(dialog).getByRole("checkbox", { name: "I accept this licence" }));
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    await screen.findByText("Everything is ready on this Mac.", {}, { timeout: 4000 });
    expect(setup().querySelector(".chip")).toBeNull();
  });
});

describe("Setup badge after a download in Settings (AC-13)", () => {
  it("drops the badge when a model download finishes while another screen is open", async () => {
    const { user, emit } = renderApp();
    const setup = () => within(mainNav()).getByRole("button", { name: "Setup" });
    await screen.findByText("Some setup items are missing.");
    expect(setup().querySelector(".chip")).toBeTruthy();

    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));
    const sheet = await screen.findByRole("dialog", { name: "Settings" });
    await user.click(within(sheet).getByRole("tab", { name: "Models" }));
    await user.click(within(sheet).getByRole("button", { name: "Download English voice" }));
    const licence = await screen.findByRole("dialog", { name: "Download English voice" });
    await user.click(within(licence).getByRole("checkbox", { name: "I accept this licence" }));
    await user.click(within(licence).getByRole("button", { name: "Download" }));
    // The shell tells the UI that the download finished.
    await emit("models://progress", { id: "enV", receivedBytes: 1, totalBytes: 1, state: "ok" });
    await user.click(within(sheet).getByRole("button", { name: "Close settings" }));

    await screen.findByText("Everything is ready on this Mac.", {}, { timeout: 4000 });
    expect(setup().querySelector(".chip")).toBeNull();
  });
});
