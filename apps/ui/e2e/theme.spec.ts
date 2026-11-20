import { expect, test, type Page } from "@playwright/test";

// AC-04: light is the default; dark follows the system; data-theme forces a theme.
const rootValue = (page: Page, name: string) =>
  page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(), name);

test.describe("light system", () => {
  test.use({ colorScheme: "light" });
  test("uses the light tokens", async ({ page }) => {
    await page.goto("/");
    expect(await rootValue(page, "--bg")).toBe("#f7f8fa");
    expect(await rootValue(page, "--acc")).toBe("#1f5f4a");
  });
});

test.describe("dark system", () => {
  test.use({ colorScheme: "dark" });
  test("uses the dark tokens", async ({ page }) => {
    await page.goto("/");
    expect(await rootValue(page, "--bg")).toBe("#101412");
    expect(await rootValue(page, "--acc")).toBe("#6fd1a8");
  });

  test("stays light when the page is forced to light", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => document.documentElement.setAttribute("data-theme", "light"));
    expect(await rootValue(page, "--bg")).toBe("#f7f8fa");
  });
});

test("the page paints the background token", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");
  const colour = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(colour).toBe("rgb(247, 248, 250)");
});
