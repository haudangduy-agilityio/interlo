// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ModelsProgressEvent } from "../../ipc/types";
import { ModelDownloadControl, type ModelDownloadControlProps } from "./ModelDownloadControl";

afterEach(cleanup);

const downloading: ModelsProgressEvent = {
  id: "enV",
  receivedBytes: 25,
  totalBytes: 100,
  state: "downloading",
};

function setup(
  props: Partial<ModelDownloadControlProps> & Pick<ModelDownloadControlProps, "state">,
) {
  const onDownload = vi.fn();
  const onCancel = vi.fn();
  const view = render(
    <ModelDownloadControl
      id="enV"
      label="English voice model"
      onDownload={onDownload}
      onCancel={onCancel}
      {...props}
    />,
  );
  return { onDownload, onCancel, ...view };
}

describe("ModelDownloadControl", () => {
  it("shows nothing for a ready model", () => {
    const { container } = setup({ state: "ok" });
    expect(container.innerHTML).toBe("");
  });

  it.each(["missing", "error"] as const)("offers Download for a %s model", async (state) => {
    const { onDownload } = setup({ state });
    await userEvent.click(screen.getByRole("button", { name: "Download English voice model" }));
    expect(onDownload).toHaveBeenCalledWith("enV");
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it.each(["downloading", "installing"] as const)(
    "shows progress and Cancel while the list says %s, before any event",
    async (state) => {
      const { onCancel } = setup({ state });
      expect(screen.getByRole("progressbar").hasAttribute("aria-valuenow")).toBe(false);
      expect(screen.queryByRole("button", { name: "Download English voice model" })).toBeNull();
      await userEvent.click(
        screen.getByRole("button", { name: "Cancel download English voice model" }),
      );
      expect(onCancel).toHaveBeenCalledWith("enV");
    },
  );

  it("shows the percent from a running event even when the list is stale", () => {
    setup({ state: "missing", event: downloading });
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("25");
    expect(screen.queryByRole("button", { name: "Download English voice model" })).toBeNull();
  });

  it("offers Download again after a failed download event", () => {
    setup({ state: "error", event: { ...downloading, state: "error" } });
    expect(screen.getByRole("button", { name: "Download English voice model" })).toBeTruthy();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });
});
