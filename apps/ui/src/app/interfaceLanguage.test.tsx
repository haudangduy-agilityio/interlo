// @vitest-environment jsdom
import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { sampleMeetings } from "../ipc/mockTransport";
import { formatDateTime } from "../shared/format";
import { renderApp } from "../shared/testing/renderApp";

const VI_SETTINGS = { uiLanguage: "vi", ducking: true, autoDeleteDays: null } as const;

async function openGeneralTab(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByRole("button", { name: "Settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Settings" });
  return {
    dialog,
    select: await within(dialog).findByRole<HTMLSelectElement>("combobox", {
      name: "Interface language",
    }),
  };
}

describe("interface language setting", () => {
  it("starts in the saved language and sets <html lang>", async () => {
    renderApp({ settings: VI_SETTINGS });
    await waitFor(() => expect(document.documentElement.lang).toBe("vi"));
    // The app still renders: a missing Vietnamese text shows the English one.
    expect(screen.getByRole("navigation")).toBeTruthy();
  });

  it("changing the setting changes <html lang> at once, and back", async () => {
    const { user } = renderApp();
    expect(document.documentElement.lang).toBe("en");
    const { select } = await openGeneralTab(user);

    await user.selectOptions(select, "vi");
    expect(document.documentElement.lang).toBe("vi");
    await user.selectOptions(select, "en");
    expect(document.documentElement.lang).toBe("en");
  });

  it("goes back to the saved language when the change is refused", async () => {
    const { user } = renderApp({
      failing: { set_setting: { code: "INVALID_INPUT", message: "Invalid value." } },
    });
    const { dialog, select } = await openGeneralTab(user);

    await user.selectOptions(select, "vi");
    expect(await within(dialog).findByRole("alert")).toBeTruthy();
    await waitFor(() => expect(document.documentElement.lang).toBe("en"));
  });

  it("dates on the Meetings screen use the saved language", async () => {
    renderApp({ screen: "meetings", settings: VI_SETTINGS });
    await waitFor(() => expect(document.documentElement.lang).toBe("vi"));
    const first = sampleMeetings()[0];
    const date = formatDateTime(first.startedAtMs, "vi");
    expect(await screen.findByText((text) => text.startsWith(date))).toBeTruthy();
    expect(date).not.toBe(formatDateTime(first.startedAtMs, "en"));
  });
});
