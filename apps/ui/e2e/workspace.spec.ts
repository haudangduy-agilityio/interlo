import { expect, test } from "@playwright/test";

test("a note with a tag is added to the list and empty text is refused", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Workspace" }).click();
  await page.getByRole("tab", { name: "Notes" }).click();

  const addButton = page.getByRole("button", { name: "Add note" });
  await expect(addButton).toBeDisabled();

  await page.getByRole("textbox", { name: "Note", exact: true }).fill("   ");
  await expect(addButton).toBeDisabled();

  await page.getByRole("textbox", { name: "Note", exact: true }).fill("Book the review room.");
  await page.getByRole("textbox", { name: "Tag (optional)" }).fill("logistics");
  await expect(addButton).toBeEnabled();
  await addButton.click();

  const notes = page.getByRole("list", { name: "Notes", exact: true });
  const row = notes.getByRole("listitem").filter({ hasText: "Book the review room." });
  await expect(row).toBeVisible();
  await expect(row.getByText("logistics")).toBeVisible();
});
