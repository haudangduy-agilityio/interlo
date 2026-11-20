import { expect, test, type Page } from "@playwright/test";

/** The Start button of the Live screen. The top bar has a Start button with the same name. */
const liveStart = (page: Page) =>
  page.getByRole("main").getByRole("button", { name: "Start meeting" });

/** Fill the "Before you start" panel so every per-meeting check passes, then confirm. */
async function confirmPanel(page: Page) {
  const panel = page.getByRole("region", { name: "Before you start" });
  await panel.getByRole("combobox", { name: "Meeting app" }).selectOption("browser");
  await panel.getByRole("checkbox", { name: "I use headphones" }).check();
  await panel
    .getByRole("checkbox", { name: "My meeting app microphone is set to Interlo" })
    .check();
  await panel.getByRole("button", { name: "Start now" }).click();
}

test("a missing model blocks Start until it is downloaded, then the meeting runs and stops", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Live meeting" }).click();
  await liveStart(page).click();
  await confirmPanel(page);

  await expect(page.getByText("Finish setup first. Not turned on yet: Models.")).toBeVisible();

  await page.getByRole("button", { name: "Open Setup" }).click();
  await expect(page.getByRole("heading", { name: "Setup" })).toBeVisible();
  await page.getByRole("button", { name: "Download English voice model" }).click();
  const licence = page.getByRole("dialog", { name: "Download English voice" });
  await expect(licence.getByRole("button", { name: "Download" })).toBeDisabled();
  await licence.getByRole("checkbox", { name: "I accept this licence" }).check();
  await licence.getByRole("button", { name: "Download" }).click();
  await expect(page.getByText("Setup is complete.")).toBeVisible();

  await page.getByRole("button", { name: "Live meeting" }).click();
  await liveStart(page).click();
  await confirmPanel(page);
  await expect(page.getByText("Meeting running.")).toBeVisible();

  await page.getByRole("button", { name: "Pause" }).click();
  await expect(
    page.getByText("Paused. Nothing is translated or played until you resume."),
  ).toBeVisible();

  await page.getByRole("button", { name: "Stop meeting" }).click();
  await expect(liveStart(page)).toBeVisible();

  await page.getByRole("button", { name: "Meetings" }).click();
  await expect(
    page.getByRole("list", { name: "Meetings" }).getByRole("button", { name: "Untitled meeting" }),
  ).toBeVisible();
});

test("Start refuses until the per-meeting checks are done, and keeps the panel open", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const sheet = page.getByRole("dialog", { name: "Settings" });
  await sheet.getByRole("tab", { name: "Models" }).click();
  await sheet.getByRole("button", { name: "Download English voice" }).click();
  const licence = page.getByRole("dialog", { name: "Download English voice" });
  await licence.getByRole("checkbox", { name: "I accept this licence" }).check();
  await licence.getByRole("button", { name: "Download" }).click();
  await sheet.getByRole("button", { name: "Close settings" }).click();

  await page.getByRole("button", { name: "Live meeting" }).click();
  await liveStart(page).click();
  const panel = page.getByRole("region", { name: "Before you start" });
  await panel.getByRole("button", { name: "Start now" }).click();

  const alert = page.getByRole("alert").filter({ hasText: "Confirm before starting" });
  await expect(alert.getByRole("listitem")).toHaveCount(3);
  await expect(panel).toBeVisible();

  await confirmPanel(page);
  await expect(page.getByText("Meeting running.")).toBeVisible();
});
