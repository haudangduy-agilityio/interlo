import { describe, expect, it, vi } from "vitest";
import { showShellWindow } from "./showWindow";

describe("showShellWindow", () => {
  it("shows the window when the page runs inside the shell", async () => {
    const show = vi.fn(() => Promise.resolve());
    await showShellWindow({ inShell: () => true, getWindow: () => ({ show }) });
    expect(show).toHaveBeenCalledTimes(1);
  });

  it("does nothing in a browser, where there is no shell window", async () => {
    const getWindow = vi.fn(() => ({ show: () => Promise.resolve() }));
    await showShellWindow({ inShell: () => false, getWindow });
    expect(getWindow).not.toHaveBeenCalled();
  });

  it("does not throw when the shell refuses to show the window", async () => {
    const show = () => Promise.reject(new Error("not allowed"));
    await expect(
      showShellWindow({ inShell: () => true, getWindow: () => ({ show }) }),
    ).resolves.toBeUndefined();
  });
});
