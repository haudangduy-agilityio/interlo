// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { sampleMeetings } from "../../ipc/mockTransport";
import { MeetingDetailView } from "./MeetingDetailView";

afterEach(cleanup);

const noop = () => {};

const DELETE_TEXT =
  "This removes the transcript, translations, summary, action items and the notes you created in this meeting. Notes not tied to a meeting stay. It cannot be undone.";

function renderDetail() {
  const user = userEvent.setup();
  render(
    <MeetingDetailView
      meeting={{ status: "done", data: sampleMeetings()[0] }}
      onBack={noop}
      onRetry={noop}
    />,
  );
  return user;
}

describe("Meeting delete dialog", () => {
  it("Delete meeting opens a dialog with the full text of what is removed and what stays", async () => {
    const user = renderDetail();
    expect(screen.queryByRole("dialog")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Delete meeting" }));
    const dialog = screen.getByRole("dialog", { name: "Delete this meeting?" });
    expect(within(dialog).getByText(DELETE_TEXT)).toBeTruthy();
  });

  it("Cancel closes the dialog and gives focus back to Delete meeting", async () => {
    const user = renderDetail();
    const opener = screen.getByRole("button", { name: "Delete meeting" });
    await user.click(opener);
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("the Delete button is off, and the dialog says why, because no command deletes yet", async () => {
    const user = renderDetail();
    await user.click(screen.getByRole("button", { name: "Delete meeting" }));
    const dialog = screen.getByRole("dialog");

    const confirm = within(dialog).getByRole("button", { name: "Delete" }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    expect(within(dialog).getByText("Deleting is not available yet.")).toBeTruthy();
  });
});
