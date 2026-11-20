// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../shared/testing/renderApp";

// AC-27: keyboard and screen reader basics of the shell and Home.
const css = readFileSync(resolve(process.cwd(), "src/styles.css"), "utf8");

describe("shell accessibility (AC-27)", () => {
  it("gives every icon-only control an accessible name", async () => {
    renderApp();
    await screen.findByRole("toolbar", { name: "Meeting controls" });
    const iconOnly = [...document.querySelectorAll<HTMLElement>("button, a")].filter(
      (el) => el.querySelector("svg") && !(el.textContent ?? "").trim(),
    );
    expect(iconOnly.length).toBeGreaterThanOrEqual(3);
    for (const el of iconOnly) {
      expect(el.getAttribute("aria-label"), el.outerHTML).toBeTruthy();
    }
  });

  it("puts the sidebar before the top bar, and the top bar before the content", async () => {
    renderApp();
    const side = (await screen.findByRole("navigation", { name: "Main" })).closest("aside");
    const bar = screen.getByRole("toolbar", { name: "Meeting controls" });
    const main = screen.getByRole("main");
    const before = (a: Node, b: Node) =>
      Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
    expect(before(side as Node, bar)).toBe(true);
    expect(before(bar, main)).toBe(true);
  });

  it("names the toolbar's controls so a screen reader can tell them apart", async () => {
    renderApp();
    const bar = within(await screen.findByRole("toolbar", { name: "Meeting controls" }));
    for (const name of ["Swap languages", "More start options", "Compact window"]) {
      expect(bar.getByRole(name === "Compact window" ? "link" : "button", { name })).toBeTruthy();
    }
  });

  it("takes over the look of a select, so WebKit honours its padding", () => {
    const rule = /\.field select \{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(rule).toContain("appearance: none");
    expect(rule).toContain("padding-right");
  });

  it("draws the focus ring as a solid 2 px accent ring with a 2 px offset", () => {
    const rule = /:focus-visible\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(rule.replace(/\s+/g, " ")).toContain("outline: 2px solid var(--acc)");
    expect(rule.replace(/\s+/g, " ")).toContain("outline-offset: 2px");
  });

  it("stops the live dot under reduced motion (the app has no spinner)", () => {
    const block = /@media \(prefers-reduced-motion: reduce\)\s*\{([^}]*\{[^}]*\}[^}]*)\}/.exec(css);
    expect(block?.[1]).toContain(".dot--live::after");
    expect(block?.[1]).toContain("animation: none");
  });
});
