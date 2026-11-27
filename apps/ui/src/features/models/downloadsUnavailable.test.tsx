// @vitest-environment jsdom
import { cleanup, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../shared/testing/renderApp";
import { sampleModels } from "../../ipc/mockTransport";

const sentence = "Downloading models is not available yet.";
const downloadFix = "Download this model, then choose Check again.";

const rowOf = (label: string) =>
  screen.getByText(label, { selector: "li > span" }).closest("li") as HTMLElement;

function expectLockedRow(label: string) {
  const row = within(rowOf(label));
  const download = row.getByRole<HTMLButtonElement>("button", { name: `Download ${label}` });
  expect(download.disabled).toBe(true);
  const hint = document.getElementById(download.getAttribute("aria-describedby") ?? "");
  expect(hint?.textContent).toBe("Not available yet");
  expect(row.getByText(sentence)).toBeTruthy();
  expect(row.queryByText(downloadFix)).toBeNull();
}

async function openModelsTab(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByRole("button", { name: "Settings" }));
  await user.click(screen.getByRole("tab", { name: "Models" }));
}

describe("model download availability", () => {
  it("app window setup locks every model download", async () => {
    renderApp({
      screen: "setup",
      kind: "tauri",
      models: sampleModels().map((m) => ({ ...m, state: "missing" })),
    });
    await screen.findByText("Speech recognition model", { selector: "li > span" });
    for (const label of [
      "Speech recognition model",
      "Translation model",
      "English voice model",
      "Vietnamese voice model",
    ]) {
      expectLockedRow(label);
    }
  });

  it("app window setup locks a failed model row", async () => {
    const stateOf = (id: string) =>
      id === "sr" ? "error" : id === "tr" ? "downloading" : ("ok" as const);
    renderApp({
      screen: "setup",
      kind: "tauri",
      devices: { mic: "missing" },
      models: sampleModels().map((m) => ({ ...m, state: stateOf(m.id) })),
    });
    await screen.findByText("Speech recognition model", { selector: "li > span" });
    expectLockedRow("Speech recognition model");

    const device = within(rowOf("Microphone"));
    expect(
      device.getByText(
        "Allow microphone access for Interlo in System Settings, then choose Check again.",
      ),
    ).toBeTruthy();
    expect(device.queryByText(sentence)).toBeNull();

    const installing = within(rowOf("Translation model"));
    expect(
      installing.getByText("Wait until the install finishes, then choose Check again."),
    ).toBeTruthy();
    expect(installing.queryByText(sentence)).toBeNull();
  });

  it("app window setup press opens no dialog and sends no download", async () => {
    const { user, transport } = renderApp({
      screen: "setup",
      kind: "tauri",
      models: sampleModels().map((m) => ({ ...m, state: "missing" })),
    });
    const invoke = vi.spyOn(transport, "invoke");
    await user.click(
      await screen.findByRole("button", { name: "Download Speech recognition model" }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    const commands = invoke.mock.calls.map(([command]) => command);
    // An unlocked press reads the model list for the licence before it opens the dialog.
    expect(commands).not.toContain("list_models");
    expect(commands).not.toContain("download_model");
  });

  it("app window models tab explains instead of failing", async () => {
    const { user } = renderApp({
      kind: "tauri",
      failing: { list_models: { code: "NOT_IMPLEMENTED", message: "Not implemented" } },
    });
    await openModelsTab(user);
    const panel = within(screen.getByRole("tabpanel"));
    expect(panel.getByText("Not available yet", { selector: ".empty-state__title" })).toBeTruthy();
    expect(panel.getByText(sentence)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    expect(panel.queryByText("Not implemented")).toBeNull();
  });

  it("app window models tab keeps other errors", async () => {
    const { user } = renderApp({
      kind: "tauri",
      failing: { list_models: { code: "INVALID_INPUT", message: "Model list failed" } },
    });
    await openModelsTab(user);
    const panel = within(screen.getByRole("tabpanel"));
    expect(panel.getByText("Model list failed")).toBeTruthy();
    expect(panel.getByRole("button", { name: "Retry" })).toBeTruthy();
    expect(panel.queryByText(sentence)).toBeNull();
    expect(panel.queryByText("Not available yet", { selector: ".empty-state__title" })).toBeNull();
  });

  it("app window models list shows locked buttons", async () => {
    const { user } = renderApp({
      kind: "tauri",
      models: sampleModels().map((m) => ({ ...m, state: m.id === "sr" ? "error" : "missing" })),
    });
    await openModelsTab(user);
    const buttons = await screen.findAllByRole("button", { name: /^Download / });
    expect(buttons).toHaveLength(sampleModels().length);
    expect(buttons.every((button) => (button as HTMLButtonElement).disabled)).toBe(true);
    expect(screen.getAllByText(sentence)).toHaveLength(1);
    const list = screen.getByRole("list", { name: "Models" });
    expect(
      screen.getByText(sentence).compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    cleanup();
    const ready = renderApp({
      kind: "tauri",
      models: sampleModels().map((m) => ({ ...m, state: "ok" })),
    });
    await openModelsTab(ready.user);
    await screen.findByRole("list", { name: "Models" });
    expect(screen.getAllByText(sentence)).toHaveLength(1);
  });

  it("browser preview keeps downloads", async () => {
    const { user } = renderApp({
      screen: "setup",
      models: sampleModels().map((m) => ({ ...m, state: m.id === "enV" ? "missing" : "ok" })),
    });
    const button = await screen.findByRole("button", { name: "Download English voice model" });
    expect((button as HTMLButtonElement).disabled).toBe(false);
    await user.click(button);
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.queryByText(sentence)).toBeNull();
    await user.keyboard("{Escape}");
    await openModelsTab(user);
    const inSettings = await screen.findByRole<HTMLButtonElement>("button", {
      name: "Download English voice",
    });
    expect(inSettings.disabled).toBe(false);
    await user.click(inSettings);
    expect(await screen.findByRole("dialog", { name: "Download English voice" })).toBeTruthy();
    expect(screen.queryByText(sentence)).toBeNull();

    cleanup();
    const failed = renderApp({
      failing: { list_models: { code: "NOT_IMPLEMENTED", message: "Not implemented" } },
    });
    await openModelsTab(failed.user);
    expect(await screen.findByRole("button", { name: "Retry" })).toBeTruthy();
    expect(screen.queryByText(sentence)).toBeNull();
  });
});
