import { expect, test } from "@playwright/test";

test("a search with no match shows an empty state and Clear search restores the list", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Meetings" }).click();

  const list = page.getByRole("list", { name: "Meetings" });
  await expect(list.getByRole("button", { name: "Weekly planning" })).toBeVisible();

  await page.getByRole("searchbox", { name: "Search meetings" }).fill("zzzz-no-such-meeting");
  await expect(page.getByText("No meetings match")).toBeVisible();
  await expect(list).toHaveCount(0);

  await page.getByRole("button", { name: "Clear search" }).click();
  await expect(page.getByText("No meetings match")).toHaveCount(0);
  await expect(list.getByRole("button", { name: "Weekly planning" })).toBeVisible();
  await expect(list.getByRole("button", { name: "Vendor call" })).toBeVisible();
});

test("Delete meeting shows what is removed and what stays, and Cancel closes it", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Meetings" }).click();
  await page
    .getByRole("list", { name: "Meetings" })
    .getByRole("button", { name: "Weekly planning" })
    .click();

  await page.getByRole("button", { name: "Delete meeting" }).click();
  const dialog = page.getByRole("dialog", { name: "Delete this meeting?" });
  await expect(
    dialog.getByText(
      "This removes the transcript, translations, summary, action items and the notes you created in this meeting. Notes not tied to a meeting stay. It cannot be undone.",
    ),
  ).toBeVisible();

  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toHaveCount(0);
});
