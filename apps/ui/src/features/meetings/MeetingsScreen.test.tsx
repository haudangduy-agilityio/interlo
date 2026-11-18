// @vitest-environment jsdom
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../shared/testing/renderApp";

describe("MeetingsScreen", () => {
  it("shows 'No meetings match' for a search miss and restores the list on Clear search", async () => {
    const { user } = renderApp({ screen: "meetings" });
    await screen.findByRole("list", { name: "Meetings" });

    await user.type(
      screen.getByRole("searchbox", { name: "Search meetings" }),
      "zzz-no-such-title",
    );
    expect(await screen.findByText("No meetings match")).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Meetings" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Clear search" }));
    const list = await screen.findByRole("list", { name: "Meetings" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(4);
    expect(
      (screen.getByRole("searchbox", { name: "Search meetings" }) as HTMLInputElement).value,
    ).toBe("");
    expect(screen.queryByText("No meetings match")).toBeNull();
  });

  it("shows 'No meetings yet' without a Clear search button when the library is empty", async () => {
    renderApp({ screen: "meetings", meetings: [] });

    expect(await screen.findByText("No meetings yet")).toBeTruthy();
    expect(screen.queryByText("No meetings match")).toBeNull();
    expect(screen.queryByRole("button", { name: "Clear search" })).toBeNull();
  });
});
