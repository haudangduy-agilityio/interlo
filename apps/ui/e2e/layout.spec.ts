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
