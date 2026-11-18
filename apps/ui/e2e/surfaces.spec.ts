import { expect, test } from "@playwright/test";

const SIDEBAR = { role: "navigation", name: "Main" } as const;

test("the subtitle bar surface renders without the sidebar", async ({ page }) => {
  await page.goto("/#surface=subtitle-bar");
  await expect(page.getByRole("region", { name: "Subtitle bar" })).toBeVisible();
  await expect(page.getByRole(SIDEBAR.role, { name: SIDEBAR.name })).toHaveCount(0);
});

test("the quick actions surface renders without the sidebar", async ({ page }) => {
  await page.goto("/#surface=quick-actions");
  await expect(page.getByRole("heading", { name: "Quick actions" })).toBeVisible();
  await expect(page.getByRole(SIDEBAR.role, { name: SIDEBAR.name })).toHaveCount(0);
});
