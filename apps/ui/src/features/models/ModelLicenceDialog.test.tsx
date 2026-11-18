// @vitest-environment jsdom
import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { sampleModels } from "../../ipc/mockTransport";
import type { ModelInfo } from "../../ipc/types";
import { renderApp, type RenderAppOptions } from "../../shared/testing/renderApp";

const withStates = (states: Record<string, ModelInfo["state"]>): ModelInfo[] =>
  sampleModels().map((m) => ({ ...m, state: states[m.id] ?? "ok" }));

const calls = (invoke: { mock: { calls: unknown[][] } }, cmd: string) =>
  invoke.mock.calls.filter(([name]) => name === cmd).map(([, args]) => args);

async function openSettingsModels(options: RenderAppOptions) {
  const app = renderApp(options);
  await app.user.click(screen.getByRole("button", { name: "Settings" }));
  const sheet = await screen.findByRole("dialog", { name: "Settings" });
  await app.user.click(within(sheet).getByRole("tab", { name: "Models" }));
  return { ...app, sheet };
}

describe("model licence dialog in Settings", () => {
  it("Download opens a dialog with the model name, a licence line and an unticked checkbox", async () => {
    const { user, sheet, transport } = await openSettingsModels({
      models: withStates({ enV: "missing" }),
    });
    const invoke = vi.spyOn(transport, "invoke");
    await user.click(await within(sheet).findByRole("button", { name: "Download English voice" }));

    const dialog = await screen.findByRole("dialog", { name: "Download English voice" });
    expect(within(dialog).getByText("Licence: Permissive open licence")).toBeTruthy();
    expect(within(dialog).queryByText("Non-commercial data")).toBeNull();
    const accept = within(dialog).getByRole("checkbox", {
      name: "I accept this licence",
    }) as HTMLInputElement;
    expect(accept.checked).toBe(false);
    expect(
      (within(dialog).getByRole("button", { name: "Download" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(calls(invoke, "download_model")).toEqual([]);
  });

  it("the Download confirm stays disabled until the checkbox is ticked, then downloads", async () => {
    const { user, sheet, transport } = await openSettingsModels({
      models: withStates({ enV: "missing" }),
    });
    const invoke = vi.spyOn(transport, "invoke");
    await user.click(await within(sheet).findByRole("button", { name: "Download English voice" }));
    const dialog = await screen.findByRole("dialog", { name: "Download English voice" });

    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(calls(invoke, "download_model")).toEqual([]);

    await user.click(within(dialog).getByRole("checkbox", { name: "I accept this licence" }));
    const confirm = within(dialog).getByRole("button", { name: "Download" }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(false);
    await user.click(confirm);

    expect(screen.queryByRole("dialog", { name: "Download English voice" })).toBeNull();
    expect(calls(invoke, "download_model")).toEqual([{ id: "enV", licenceAccepted: true }]);
    await waitFor(() =>
      expect(within(sheet).queryByRole("button", { name: /^Download/ })).toBeNull(),
    );
  });

  it("a model with the non-commercial flag shows the chip; one without does not", async () => {
    const { user, sheet } = await openSettingsModels({
      models: withStates({ tr: "missing", enV: "missing" }),
    });
    await user.click(await within(sheet).findByRole("button", { name: "Download Translation" }));
    const flagged = await screen.findByRole("dialog", { name: "Download Translation" });
    expect(within(flagged).getByText("Non-commercial data")).toBeTruthy();
    await user.click(within(flagged).getByRole("button", { name: "Cancel" }));

    await user.click(within(sheet).getByRole("button", { name: "Download English voice" }));
    const plain = await screen.findByRole("dialog", { name: "Download English voice" });
    expect(within(plain).queryByText("Non-commercial data")).toBeNull();
  });

  it("Cancel downloads nothing, closes the dialog and gives focus back", async () => {
    const { user, sheet, transport } = await openSettingsModels({
      models: withStates({ enV: "missing" }),
    });
    const invoke = vi.spyOn(transport, "invoke");
    const opener = await within(sheet).findByRole("button", { name: "Download English voice" });
    await user.click(opener);
    const dialog = await screen.findByRole("dialog", { name: "Download English voice" });
    await user.click(within(dialog).getByRole("checkbox", { name: "I accept this licence" }));
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog", { name: "Download English voice" })).toBeNull();
    expect(calls(invoke, "download_model")).toEqual([]);
    expect(document.activeElement).toBe(opener);
  });

  it("Escape closes only the licence dialog, not the Settings sheet", async () => {
    const { user, sheet } = await openSettingsModels({ models: withStates({ enV: "missing" }) });
    await user.click(await within(sheet).findByRole("button", { name: "Download English voice" }));
    await screen.findByRole("dialog", { name: "Download English voice" });

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Download English voice" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeTruthy();
  });

  it("asks again the next time: the checkbox starts unticked on every open", async () => {
    const { user, sheet } = await openSettingsModels({ models: withStates({ enV: "missing" }) });
    const open = async () => {
      await user.click(
        await within(sheet).findByRole("button", { name: "Download English voice" }),
      );
      return screen.findByRole("dialog", { name: "Download English voice" });
    };
    const first = await open();
    await user.click(within(first).getByRole("checkbox", { name: "I accept this licence" }));
    await user.click(within(first).getByRole("button", { name: "Cancel" }));

    const second = await open();
    expect(
      (within(second).getByRole("checkbox", { name: "I accept this licence" }) as HTMLInputElement)
        .checked,
    ).toBe(false);
  });
});

describe("model licence text from the model list", () => {
  it("shows the licence line the shell sent, and the chip only for a non-commercial model", async () => {
    const models = withStates({ enV: "missing", viV: "missing" }).map((m) =>
      m.id === "enV"
        ? { ...m, licence: "Custom licence line for the English voice", nonCommercial: false }
        : m.id === "viV"
          ? { ...m, licence: "CC BY-NC data", nonCommercial: true }
          : m,
    );
    const { user, sheet } = await openSettingsModels({ models });
    await user.click(await within(sheet).findByRole("button", { name: "Download English voice" }));
    const plain = await screen.findByRole("dialog", { name: "Download English voice" });
    expect(
      within(plain).getByText("Licence: Custom licence line for the English voice"),
    ).toBeTruthy();
    expect(within(plain).queryByText("Non-commercial data")).toBeNull();
    await user.click(within(plain).getByRole("button", { name: "Cancel" }));

    await user.click(within(sheet).getByRole("button", { name: "Download Vietnamese voice" }));
    const flagged = await screen.findByRole("dialog", { name: "Download Vietnamese voice" });
    expect(within(flagged).getByText("Licence: CC BY-NC data")).toBeTruthy();
    expect(within(flagged).getByText("Non-commercial data")).toBeTruthy();
  });

  it("says the licence is not listed only when the licence is an empty string", async () => {
    const models = withStates({ enV: "missing", viV: "missing" }).map((m) =>
      m.id === "enV" ? { ...m, licence: "" } : m,
    );
    const { user, sheet } = await openSettingsModels({ models });
    await user.click(await within(sheet).findByRole("button", { name: "Download English voice" }));
    const empty = await screen.findByRole("dialog", { name: "Download English voice" });
    expect(within(empty).getByText("Licence: not listed.")).toBeTruthy();
    await user.click(within(empty).getByRole("button", { name: "Cancel" }));

    await user.click(within(sheet).getByRole("button", { name: "Download Vietnamese voice" }));
    const listed = await screen.findByRole("dialog", { name: "Download Vietnamese voice" });
    expect(within(listed).queryByText(/not listed/)).toBeNull();
  });

  it("sends licenceAccepted true only after the tick, never before", async () => {
    const { user, sheet, transport } = await openSettingsModels({
      models: withStates({ enV: "missing" }),
    });
    const invoke = vi.spyOn(transport, "invoke");
    await user.click(await within(sheet).findByRole("button", { name: "Download English voice" }));
    const dialog = await screen.findByRole("dialog", { name: "Download English voice" });
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(calls(invoke, "download_model")).toEqual([]);
    await user.click(within(dialog).getByRole("checkbox", { name: "I accept this licence" }));
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(calls(invoke, "download_model")).toEqual([{ id: "enV", licenceAccepted: true }]);
  });
});

describe("model licence dialog on Setup", () => {
  it("the Download button on a Setup row asks for the licence before it downloads", async () => {
    const { user, transport } = renderApp({ screen: "setup" });
    const invoke = vi.spyOn(transport, "invoke");
    await user.click(await screen.findByRole("button", { name: "Download English voice model" }));

    const dialog = await screen.findByRole("dialog", { name: "Download English voice" });
    expect(calls(invoke, "download_model")).toEqual([]);
    await user.click(within(dialog).getByRole("checkbox", { name: "I accept this licence" }));
    await user.click(within(dialog).getByRole("button", { name: "Download" }));

    expect(await screen.findByText("Setup is complete.")).toBeTruthy();
    expect(calls(invoke, "download_model")).toEqual([{ id: "enV", licenceAccepted: true }]);
  });

  it("still asks for acceptance when the model list cannot be read", async () => {
    const { user } = renderApp({
      screen: "setup",
      failing: { list_models: "Could not read models." },
    });
    await user.click(await screen.findByRole("button", { name: "Download English voice model" }));

    const dialog = await screen.findByRole("dialog", { name: "Download English voice model" });
    expect(within(dialog).getByText("Licence: not listed.")).toBeTruthy();
    expect(
      (within(dialog).getByRole("button", { name: "Download" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });
});
