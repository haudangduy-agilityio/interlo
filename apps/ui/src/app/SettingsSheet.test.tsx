// @vitest-environment jsdom
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../shared/testing/renderApp";

describe("settings sheet focus", () => {
  it("marks the page behind it inert while open, and clears that on close", async () => {
    const { user } = renderApp();
    const side = screen.getByRole("navigation", { name: "Main" }).closest("aside") as HTMLElement;
    const page = screen.getByRole("main").parentElement as HTMLElement;
    expect(side.hasAttribute("inert")).toBe(false);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    expect(side.hasAttribute("inert")).toBe(true);
    expect(page.hasAttribute("inert")).toBe(true);
    expect(dialog.closest("[inert]")).toBeNull();

    await user.click(within(dialog).getByRole("button", { name: "Close settings" }));
    expect(side.hasAttribute("inert")).toBe(false);
    expect(page.hasAttribute("inert")).toBe(false);
  });

  it("Tab wraps inside the sheet and never reaches the page behind", async () => {
    const { user } = renderApp();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    await within(dialog).findByRole("combobox", { name: "Interface language" });

    const inside = (el: Element | null) => el !== null && dialog.contains(el);
    for (let n = 0; n < 15; n += 1) {
      await user.tab();
      expect(inside(document.activeElement)).toBe(true);
    }
    for (let n = 0; n < 15; n += 1) {
      await user.tab({ shift: true });
      expect(inside(document.activeElement)).toBe(true);
    }
  });

  it("Tab wraps inside the model licence dialog that opens over the sheet", async () => {
    const { user } = renderApp();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const sheet = await screen.findByRole("dialog", { name: "Settings" });
    await user.click(within(sheet).getByRole("tab", { name: "Models" }));
    await user.click(await within(sheet).findByRole("button", { name: "Download English voice" }));
    const licence = await screen.findByRole("dialog", { name: "Download English voice" });

    for (let n = 0; n < 8; n += 1) {
      await user.tab();
      expect(licence.contains(document.activeElement)).toBe(true);
    }
    // The licence dialog handled Tab, so the sheet underneath did not move focus.
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Download English voice" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeTruthy();
  });

  it("arrow keys move between the settings tabs", async () => {
    const { user } = renderApp();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const sheet = await screen.findByRole("dialog", { name: "Settings" });
    within(sheet).getByRole("tab", { name: "General" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(within(sheet).getByRole("tab", { name: "Voices" }));
    expect(within(sheet).getByRole("tabpanel").getAttribute("id")).toBe("panel-voices");
  });
});
