// @vitest-environment jsdom
import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { sampleModels } from "../../ipc/mockTransport";
import type { ModelInfo } from "../../ipc/types";
import { acceptLicenceAndDownload } from "../../shared/testing/flows";
import { renderApp, type RenderAppOptions } from "../../shared/testing/renderApp";

const withStates = (states: Record<string, ModelInfo["state"]>): ModelInfo[] =>
  sampleModels().map((m) => ({ ...m, state: states[m.id] ?? "ok" }));

const calls = (invoke: { mock: { calls: unknown[][] } }, cmd: string) =>
  invoke.mock.calls.filter(([name]) => name === cmd).map(([, args]) => args);

const progress = (patch: Record<string, unknown>) => ({
  id: "enV",
  receivedBytes: 25,
  totalBytes: 100,
  state: "downloading",
  ...patch,
});

const setupRow = (label: string) =>
  screen.getByText(label, { selector: "li > span" }).closest("li") as HTMLElement;

async function openSettingsModels(options: RenderAppOptions) {
  const app = renderApp(options);
  await app.user.click(screen.getByRole("button", { name: "Settings" }));
  const sheet = await screen.findByRole("dialog", { name: "Settings" });
  await app.user.click(within(sheet).getByRole("tab", { name: "Models" }));
  await within(sheet).findByRole("list", { name: "Models" });
  return { ...app, sheet };
}

describe("Setup model rows show download progress", () => {
  it("shows a progress bar with the percent while the model downloads, and no Download button", async () => {
    const { emit } = renderApp({ screen: "setup" });
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({ receivedBytes: 25, totalBytes: 100 }));

    const row = setupRow("English voice model");
    const bar = within(row).getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBe("25");
    expect((bar.firstElementChild as HTMLElement).style.width).toBe("25%");
    expect(bar.getAttribute("aria-valuemin")).toBe("0");
    expect(bar.getAttribute("aria-valuemax")).toBe("100");
    expect(within(row).queryByRole("button", { name: "Download English voice model" })).toBeNull();
    expect(
      within(row).getByRole("button", { name: "Cancel download English voice model" }),
    ).toBeTruthy();
  });

  it("follows later events and keeps the bar while the file is verified", async () => {
    const { emit } = renderApp({ screen: "setup" });
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({ receivedBytes: 50 }));
    expect(
      within(setupRow("English voice model"))
        .getByRole("progressbar")
        .getAttribute("aria-valuenow"),
    ).toBe("50");
    await emit("models://progress", progress({ receivedBytes: 100, state: "verifying" }));
    const bar = within(setupRow("English voice model")).getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBe("100");
    expect(within(setupRow("English voice model")).getByText("Verifying")).toBeTruthy();
  });

  it("shows a bar without a value when the total size is not known", async () => {
    const { emit } = renderApp({ screen: "setup" });
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({ receivedBytes: 10, totalBytes: 0 }));
    const bar = within(setupRow("English voice model")).getByRole("progressbar");
    expect(bar.hasAttribute("aria-valuenow")).toBe(false);
  });

  it("shows a bar for a model the list reports as downloading, before any event", async () => {
    renderApp({ screen: "setup", models: withStates({ tr: "downloading" }) });
    const row = (await screen.findByText("Translation model", { selector: "li > span" })).closest(
      "li",
    ) as HTMLElement;
    expect(within(row).getByRole("progressbar").hasAttribute("aria-valuenow")).toBe(false);
    expect(
      within(row).getByRole("button", { name: "Cancel download Translation model" }),
    ).toBeTruthy();
  });

  it("has no bar for a model that is not downloading", async () => {
    renderApp({ screen: "setup" });
    await screen.findByRole("button", { name: "Download English voice model" });
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("Cancel calls cancel_model_download with the id and removes the bar", async () => {
    const { user, emit, transport } = renderApp({ screen: "setup" });
    const invoke = vi.spyOn(transport, "invoke");
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({}));
    await user.click(screen.getByRole("button", { name: "Cancel download English voice model" }));

    expect(calls(invoke, "cancel_model_download")).toEqual([{ id: "enV" }]);
    await waitFor(() => expect(screen.queryByRole("progressbar")).toBeNull());
    expect(
      await screen.findByRole("button", { name: "Download English voice model" }),
    ).toBeTruthy();
  });

  it("a refused cancel shows an alert on the page and keeps the bar", async () => {
    const { user, emit } = renderApp({
      screen: "setup",
      failing: {
        cancel_model_download: {
          code: "NOT_IMPLEMENTED",
          message: "Cancelling a model download is not available yet.",
        },
      },
    });
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({}));
    await user.click(screen.getByRole("button", { name: "Cancel download English voice model" }));

    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByText("Cancelling a model download is not available yet."),
    ).toBeTruthy();
    expect(screen.getByRole("progressbar")).toBeTruthy();
  });

  it("an error event shows the failure on the page with Retry, and Retry downloads again", async () => {
    const { user, emit, transport } = renderApp({ screen: "setup" });
    const invoke = vi.spyOn(transport, "invoke");
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({ state: "error" }));

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Could not download English voice model.")).toBeTruthy();
    expect(screen.queryByRole("progressbar")).toBeNull();

    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(calls(invoke, "download_model")).toEqual([{ id: "enV", licenceAccepted: true }]);
    expect(await screen.findByText("Setup is complete.")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("dismissing the error removes it without a new download", async () => {
    const { user, emit, transport } = renderApp({ screen: "setup" });
    const invoke = vi.spyOn(transport, "invoke");
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({ state: "error" }));
    await user.click(await screen.findByRole("button", { name: "Dismiss error" }));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(calls(invoke, "download_model")).toEqual([]);
  });

  it("a finished download (ok event) loads the setup list again", async () => {
    const { emit, transport } = renderApp({ screen: "setup" });
    await screen.findByRole("button", { name: "Download English voice model" });
    // The shell finished the download without the UI asking now (for example a resumed one).
    await transport.invoke("download_model", { id: "enV", licenceAccepted: true });
    await emit("models://progress", progress({ state: "ok", receivedBytes: 100 }));
    expect(await screen.findByText("Setup is complete.")).toBeTruthy();
  });

  it("ignores an event for an id it does not know", async () => {
    const { emit } = renderApp({ screen: "setup" });
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({ id: "mic" }));
    expect(screen.queryByRole("progressbar")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("an accepted download from the dialog still works with progress events on", async () => {
    const { user, emit } = renderApp({ screen: "setup" });
    await user.click(await screen.findByRole("button", { name: "Download English voice model" }));
    await acceptLicenceAndDownload(user);
    expect(await screen.findByText("Setup is complete.")).toBeTruthy();
    await emit("models://progress", progress({ state: "ok", receivedBytes: 100 }));
    expect(screen.queryByRole("progressbar")).toBeNull();
  });
});

describe("Settings Models rows show download progress", () => {
  it("shows a progress bar and a Cancel button while the model downloads", async () => {
    const { emit, sheet } = await openSettingsModels({ models: withStates({ enV: "missing" }) });
    await within(sheet).findByRole("button", { name: "Download English voice" });
    await emit("models://progress", progress({ receivedBytes: 40 }));

    const bar = within(sheet).getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBe("40");
    expect(within(sheet).queryByRole("button", { name: "Download English voice" })).toBeNull();
    expect(
      within(sheet).getByRole("button", { name: "Cancel download English voice" }),
    ).toBeTruthy();
  });

  it("shows the bar for a model the list reports as downloading", async () => {
    const { sheet } = await openSettingsModels({ models: withStates({ tr: "downloading" }) });
    expect(within(sheet).getByRole("progressbar")).toBeTruthy();
    expect(within(sheet).getByRole("button", { name: "Cancel download Translation" })).toBeTruthy();
  });

  it("Cancel calls cancel_model_download with the id", async () => {
    const { user, emit, sheet, transport } = await openSettingsModels({
      models: withStates({ enV: "missing" }),
    });
    const invoke = vi.spyOn(transport, "invoke");
    await within(sheet).findByRole("button", { name: "Download English voice" });
    await emit("models://progress", progress({}));
    await user.click(within(sheet).getByRole("button", { name: "Cancel download English voice" }));
    expect(calls(invoke, "cancel_model_download")).toEqual([{ id: "enV" }]);
    await waitFor(() => expect(within(sheet).queryByRole("progressbar")).toBeNull());
  });

  it("an error event shows the failure inside the sheet with Retry", async () => {
    const { user, emit, sheet, transport } = await openSettingsModels({
      models: withStates({ enV: "missing" }),
    });
    const invoke = vi.spyOn(transport, "invoke");
    await within(sheet).findByRole("button", { name: "Download English voice" });
    await emit("models://progress", progress({ state: "error" }));

    const alert = await within(sheet).findByRole("alert");
    expect(within(alert).getByText("Could not download English voice.")).toBeTruthy();
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(calls(invoke, "download_model")).toEqual([{ id: "enV", licenceAccepted: true }]);
    await waitFor(() => expect(within(sheet).queryByRole("alert")).toBeNull());
  });

  it("a finished download (ok event) loads the list again", async () => {
    const { emit, sheet, transport } = await openSettingsModels({
      models: withStates({ enV: "missing" }),
    });
    await within(sheet).findByRole("button", { name: "Download English voice" });
    await transport.invoke("download_model", { id: "enV", licenceAccepted: true });
    await emit("models://progress", progress({ state: "ok", receivedBytes: 100 }));
    await waitFor(() =>
      expect(within(sheet).queryByRole("button", { name: /^Download/ })).toBeNull(),
    );
  });
});

describe("a stale progress event", () => {
  it("shows no bar for a model the list already reports as ready", async () => {
    const { emit } = renderApp({ screen: "setup" });
    await screen.findByRole("button", { name: "Download English voice model" });
    await emit("models://progress", progress({ id: "sr" }));
    expect(screen.queryByRole("progressbar")).toBeNull();
  });
});
