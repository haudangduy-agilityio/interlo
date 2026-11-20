// @vitest-environment jsdom
import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  acceptLicenceAndDownload,
  confirmStartPanel,
  liveStartButton,
} from "../shared/testing/flows";
import { renderApp } from "../shared/testing/renderApp";

const NOT_BUILT = { code: "NOT_IMPLEMENTED", message: "Model download is not built yet." } as const;

describe("failures stay on the page", () => {
  it("Setup: a refused download shows an alert with Retry, and Retry can finish setup", async () => {
    const { user, transport } = renderApp({
      screen: "setup",
      failing: { download_model: NOT_BUILT },
    });
    await user.click(await screen.findByRole("button", { name: "Download English voice model" }));
    await acceptLicenceAndDownload(user);

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Model download is not built yet.")).toBeTruthy();

    transport.heal("download_model");
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Setup is complete.")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("Settings General: a refused change shows an alert inside the dialog", async () => {
    const { user } = renderApp({
      failing: { set_setting: { code: "INVALID_INPUT", message: "Invalid value for ducking." } },
    });
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    await user.click(await within(dialog).findByRole("checkbox"));

    const alert = await within(dialog).findByRole("alert");
    expect(within(alert).getByText("Invalid value for ducking.")).toBeTruthy();
    // The saved value is shown again, not the refused one.
    expect((await within(dialog).findByRole<HTMLInputElement>("checkbox")).checked).toBe(true);
  });

  it("Settings Models: a refused download shows an alert inside the dialog", async () => {
    const { user } = renderApp({ failing: { download_model: NOT_BUILT } });
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Models" }));
    await user.click(await within(dialog).findByRole("button", { name: "Download English voice" }));
    await acceptLicenceAndDownload(user);

    const alert = await within(dialog).findByRole("alert");
    expect(within(alert).getByText("Model download is not built yet.")).toBeTruthy();
  });

  it("Live: a refused mute and a refused stop show an alert on the screen", async () => {
    const { user, emit, transport } = renderApp({ screen: "live" });
    await emit("meeting://status", { state: "running" });

    // The mock has no meeting, so set_mute rejects with NO_MEETING.
    await user.click(await screen.findByRole("button", { name: "Mute my voice" }));
    expect(
      within(await screen.findByRole("alert")).getByText("No meeting is running."),
    ).toBeTruthy();

    transport.failCommand("stop_meeting", "Shell stopped answering.");
    await user.click(screen.getByRole("button", { name: "Stop meeting" }));
    expect(
      within(await screen.findByRole("alert")).getByText("Shell stopped answering."),
    ).toBeTruthy();
  });

  it("Compact panel: a refused mute shows an alert in the panel", async () => {
    const { user, emit } = renderApp({ surface: "compact-panel" });
    await emit("meeting://status", { state: "running" });
    await user.click(await screen.findByRole("button", { name: "Mute my voice" }));
    expect(
      within(await screen.findByRole("alert")).getByText("No meeting is running."),
    ).toBeTruthy();
  });

  it("Workspace: a refused note shows an alert and keeps the draft", async () => {
    const { user } = renderApp({
      screen: "workspace",
      failing: {
        add_note: { code: "INVALID_INPUT", message: "A note can have at most 5000 characters." },
      },
    });
    await user.click(await screen.findByRole("tab", { name: "Notes" }));
    await user.type(await screen.findByRole("textbox", { name: "Note" }), "Keep me.");
    await user.click(screen.getByRole("button", { name: "Add note" }));

    expect(
      within(await screen.findByRole("alert")).getByText(
        "A note can have at most 5000 characters.",
      ),
    ).toBeTruthy();
    expect((screen.getByRole("textbox", { name: "Note" }) as HTMLTextAreaElement).value).toBe(
      "Keep me.",
    );
  });
});

describe("Live start banner", () => {
  it("clears the setup message once setup is ready and the user comes back", async () => {
    const { user } = renderApp({ screen: "live" });
    await user.click(await liveStartButton());
    await confirmStartPanel(user);
    expect(await screen.findByText(/^Finish setup first/)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Open Setup" }));
    await user.click(await screen.findByRole("button", { name: "Download English voice model" }));
    await acceptLicenceAndDownload(user);
    expect(await screen.findByText("Setup is complete.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Live meeting" }));

    await liveStartButton();
    expect(screen.queryByText(/^Finish setup first/)).toBeNull();
  });

  it("clears the setup message when the user leaves the screen and returns", async () => {
    const { user } = renderApp({ screen: "live" });
    await user.click(await liveStartButton());
    await confirmStartPanel(user);
    expect(await screen.findByText(/^Finish setup first/)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Today" }));
    await user.click(screen.getByRole("button", { name: "Live meeting" }));
    await liveStartButton();
    expect(screen.queryByText(/^Finish setup first/)).toBeNull();
  });

  it("clears the setup message when a model is downloaded in Settings and the sheet closes", async () => {
    const { user } = renderApp({ screen: "live" });
    await user.click(await liveStartButton());
    await confirmStartPanel(user);
    expect(await screen.findByText(/^Finish setup first/)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Models" }));
    await user.click(await within(dialog).findByRole("button", { name: "Download English voice" }));
    await acceptLicenceAndDownload(user);
    await user.click(within(dialog).getByRole("button", { name: "Close settings" }));

    await liveStartButton();
    await waitFor(() => expect(screen.queryByText(/^Finish setup first/)).toBeNull());
  });
});
