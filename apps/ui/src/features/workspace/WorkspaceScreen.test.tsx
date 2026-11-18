// @vitest-environment jsdom
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../shared/testing/renderApp";
import { startOfToday } from "./WorkspaceScreen";

describe("WorkspaceScreen", () => {
  it("lists a new note with its tag after Add note, and clears the form", async () => {
    const { user } = renderApp({ screen: "workspace" });
    await user.click(await screen.findByRole("tab", { name: "Notes" }));
    await screen.findByRole("list", { name: "Notes" });

    await user.type(screen.getByRole("textbox", { name: "Note" }), "Book the demo room.");
    await user.type(screen.getByRole("textbox", { name: "Tag (optional)" }), "logistics");
    await user.click(screen.getByRole("button", { name: "Add note" }));

    const list = await screen.findByRole("list", { name: "Notes" });
    const row = within(list).getByText("Book the demo room.").closest("li") as HTMLElement;
    expect(within(row).getByText("logistics")).toBeTruthy();
    expect(within(list).getAllByRole("listitem")).toHaveLength(3);
    expect((screen.getByRole("textbox", { name: "Note" }) as HTMLTextAreaElement).value).toBe("");
    expect(screen.getByText("Note added.")).toBeTruthy();
  });

  it("keeps Add note disabled while the text is blank", async () => {
    const { user } = renderApp({ screen: "workspace" });
    await user.click(await screen.findByRole("tab", { name: "Notes" }));
    const add = screen.getByRole("button", { name: "Add note" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);

    await user.type(screen.getByRole("textbox", { name: "Note" }), "   ");
    expect(add.disabled).toBe(true);

    await user.type(screen.getByRole("textbox", { name: "Note" }), "x");
    expect(add.disabled).toBe(false);
  });
});

describe("startOfToday", () => {
  it("is local midnight, whatever the zone: 23:59 and 00:00 local share a day", () => {
    const evening = new Date(2026, 10, 17, 23, 59, 30);
    const morning = new Date(2026, 10, 17, 0, 0, 0);
    expect(startOfToday(evening)).toBe(new Date(2026, 10, 17).getTime());
    expect(startOfToday(morning)).toBe(new Date(2026, 10, 17).getTime());
    expect(startOfToday(new Date(2026, 10, 18, 0, 0, 1))).toBe(new Date(2026, 10, 18).getTime());
  });
});
