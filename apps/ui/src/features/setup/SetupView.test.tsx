// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup } from "@testing-library/react";
import { SETUP_ITEM_LABEL, setupFixText } from "../../ipc/labels";
import type { SetupItem, SetupItemState, SetupStatus } from "../../ipc/types";
import { SetupView } from "./SetupView";

afterEach(cleanup);

const noop = () => {};

function status(states: Partial<Record<SetupItem["id"], SetupItemState>>): SetupStatus {
  const row = (id: SetupItem["id"], group: SetupItem["group"]): SetupItem => ({
    id,
    group,
    state: states[id] ?? "ok",
    detail: `${id} detail`,
  });
  const items = [
    row("mic", "devices"),
    row("phones", "devices"),
    row("capture", "devices"),
    row("vmic", "virtualMicrophone"),
    row("sr", "models"),
    row("tr", "models"),
    row("enV", "models"),
    row("viV", "models"),
  ];
  return {
    items,
    canStart: items.every((i) => i.id === "phones" || i.state === "ok"),
    missingGroups: [],
  };
}

function renderSetup(s: SetupStatus) {
  render(<SetupView status={{ status: "done", data: s }} onRecheck={noop} onDownload={noop} />);
}

const rowOf = (label: string) =>
  screen.getByText(label, { selector: "li > span" }).closest("li") as HTMLElement;

describe("Setup rows", () => {
  it("has a row for Microphone, Meeting audio, Virtual microphone and every model", () => {
    renderSetup(status({}));
    for (const label of [
      "Microphone",
      "Meeting audio",
      "Virtual microphone",
      "Speech recognition model",
      "Translation model",
      "English voice model",
      "Vietnamese voice model",
    ])
      expect(rowOf(label)).toBeTruthy();
  });

  it("labels come from ipc/labels.ts", () => {
    expect(SETUP_ITEM_LABEL.mic).toBe("Microphone");
    expect(SETUP_ITEM_LABEL.capture).toBe("Meeting audio");
    expect(SETUP_ITEM_LABEL.vmic).toBe("Virtual microphone");
  });

  it.each([
    ["mic", "Microphone"],
    ["capture", "Meeting audio"],
    ["vmic", "Virtual microphone"],
    ["enV", "English voice model"],
  ] as const)("a %s row that is not ok shows a fix text", (id, label) => {
    for (const state of ["missing", "installing", "error"] as const) {
      cleanup();
      renderSetup(status({ [id]: state }));
      const fix = setupFixText({ id, state });
      expect(fix.length).toBeGreaterThan(0);
      expect(within(rowOf(label)).getByText(fix)).toBeTruthy();
    }
  });

  it("an ok row shows no fix text", () => {
    renderSetup(status({}));
    expect(screen.queryByText(/Check again\.$/)).toBeNull();
    for (const item of ["Microphone", "Meeting audio", "Virtual microphone"])
      expect(within(rowOf(item)).queryByText(/then choose Check again/)).toBeNull();
  });

  it("fix texts say what to do, in plain words", () => {
    expect(setupFixText({ id: "mic", state: "missing" })).toBe(
      "Allow microphone access for Interlo in System Settings, then choose Check again.",
    );
    expect(setupFixText({ id: "tr", state: "error" })).toBe(
      "Download this model, then choose Check again.",
    );
    expect(setupFixText({ id: "tr", state: "installing" })).toBe(
      "Wait until the install finishes, then choose Check again.",
    );
  });

  it("keeps Show setup steps for a missing virtual microphone", () => {
    renderSetup(status({ vmic: "missing" }));
    expect(screen.getByRole("button", { name: "Show setup steps" })).toBeTruthy();
  });
});
