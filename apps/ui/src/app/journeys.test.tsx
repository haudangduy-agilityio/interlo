// @vitest-environment jsdom
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  acceptLicenceAndDownload,
  confirmStartPanel,
  liveStartButton,
} from "../shared/testing/flows";
import { renderApp } from "../shared/testing/renderApp";

const SETUP_MESSAGE = "Finish setup first. Not turned on yet: Models.";

describe("start gate", () => {
  it("shows the setup message and opens Setup when a model is missing", async () => {
    const { user } = renderApp({ screen: "live" });

    await user.click(await liveStartButton());
    await confirmStartPanel(user);
    expect(await screen.findByText(SETUP_MESSAGE)).toBeTruthy();
    expect(screen.queryByText("Meeting running.")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Open Setup" }));
    expect(await screen.findByRole("heading", { name: "Setup" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Download English voice model" })).toBeTruthy();
  });

  it("runs, pauses and stops a meeting once the missing model is downloaded", async () => {
    const { user, emit } = renderApp({ screen: "setup" });

    await user.click(await screen.findByRole("button", { name: "Download English voice model" }));
    await acceptLicenceAndDownload(user);
    expect(await screen.findByText("Setup is complete.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Download/ })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Live meeting" }));
    await user.click(await liveStartButton());
    await confirmStartPanel(user);
    // The shell, not the start command, reports the running state.
    await screen.findByText("Starting meeting…");
    await emit("meeting://status", { state: "running" });
    expect(await screen.findByText("Meeting running.")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Pause" }));
    expect(
      await screen.findByText("Paused. Nothing is translated or played until you resume."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Resume" })).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Stop meeting" }));
    expect(await liveStartButton()).toBeTruthy();
    expect(screen.queryByText(/^Paused\. Nothing/)).toBeNull();

    await user.click(screen.getByRole("button", { name: "Meetings" }));
    const list = await screen.findByRole("list", { name: "Meetings" });
    const row = within(list)
      .getByRole("button", { name: "Untitled meeting" })
      .closest("li") as HTMLElement;
    expect(within(row).getByText("Generating summary")).toBeTruthy();
  });
});

describe("settings sheet", () => {
  it("opens as a dialog with focus inside and gives focus back on Escape", async () => {
    const { user } = renderApp();
    const opener = screen.getByRole("button", { name: "Settings" });

    await user.click(opener);
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    expect(dialog.contains(document.activeElement)).toBe(true);

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("gives focus back to the Settings button when Close is clicked", async () => {
    const { user } = renderApp();
    const opener = screen.getByRole("button", { name: "Settings" });

    await user.click(opener);
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    await user.click(within(dialog).getByRole("button", { name: "Close settings" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});

describe("failed command", () => {
  it("shows the error with Retry on Meetings, and Retry loads the list", async () => {
    const { user, transport } = renderApp({
      screen: "meetings",
      failing: { list_meetings: "Could not read meetings." },
    });

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Could not read meetings.")).toBeTruthy();

    transport.heal("list_meetings");
    await user.click(within(alert).getByRole("button", { name: "Retry" }));

    const list = await screen.findByRole("list", { name: "Meetings" });
    expect(within(list).getByRole("button", { name: "Weekly planning" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
