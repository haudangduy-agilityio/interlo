import { expect, test } from "@playwright/test";

// AC-28: no horizontal scrollbar and no overlapping top bar controls at 960 by 600 and at the
// minimum height of 560.
for (const size of [
  { width: 960, height: 600 },
  { width: 960, height: 560 },
]) {
  test(`Home fits at ${size.width} by ${size.height}`, async ({ page }) => {
    await page.setViewportSize(size);
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Today", level: 1 })).toBeVisible();

    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);

    const boxes = await page
      .getByRole("toolbar", { name: "Meeting controls" })
      .locator("> *")
      .evaluateAll((els) =>
        els
          .map((el) => el.getBoundingClientRect())
          .filter((r) => r.width > 0)
          .map((r) => ({ left: r.left, right: r.right })),
      );
    expect(boxes.length).toBeGreaterThanOrEqual(3);
    for (let i = 1; i < boxes.length; i++) {
      expect(boxes[i].left, `control ${i} starts after control ${i - 1}`).toBeGreaterThanOrEqual(
        boxes[i - 1].right - 0.5,
      );
    }
  });
}

// Real-window finding (M1-51, part B): below 760 px the sidebar became a row that clipped its
// entries, hid Settings, and the top bar ran out of the window. At every width below 960 px
// nothing may scroll sideways, every control must be inside the window, and Settings must stay
// reachable.
for (const width of [900, 760, 480, 360]) {
  test(`the shell has no clipped or hidden control at ${width} px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 700 });
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Today", level: 1 })).toBeVisible();

    const page_ = await page.evaluate(() => ({
      overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      sidebarOverflow: (() => {
        const sb = document.querySelector<HTMLElement>(".sb");
        return sb ? sb.scrollWidth - sb.clientWidth : -1;
      })(),
      outside: [
        ...document.querySelectorAll<HTMLElement>(
          ".sb button, .sb a, [role=toolbar] button, [role=toolbar] a",
        ),
      ]
        .filter((el) => el.getBoundingClientRect().width > 0)
        .filter((el) => el.getBoundingClientRect().right > window.innerWidth + 0.5)
        .map((el) => el.textContent?.trim() || el.getAttribute("aria-label") || el.tagName),
    }));
    expect(page_.overflow, "page scrolls sideways").toBeLessThanOrEqual(0);
    expect(page_.sidebarOverflow, "sidebar scrolls sideways").toBeLessThanOrEqual(0);
    expect(page_.outside, "controls outside the window").toEqual([]);
    await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
    await expect(
      page.getByRole("navigation", { name: "Main" }).getByRole("button", { name: "Setup" }),
    ).toBeVisible();
  });
}
