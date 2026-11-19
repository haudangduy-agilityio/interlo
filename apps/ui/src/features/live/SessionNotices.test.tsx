// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { initialSession, type SessionState } from "../../ipc/meetingSession";
import { SessionBanner, StartingNotice } from "./SessionNotices";

afterEach(cleanup);

const withBanner: SessionState = {
  ...initialSession,
  banner: { kind: "warning", message: "Network changed" },
};

describe("SessionBanner", () => {
  it("shows nothing without a banner", () => {
    const { container } = render(<SessionBanner session={initialSession} />);
    expect(container.innerHTML).toBe("");
  });

  it("shows the message without a dismiss button when no handler is given", () => {
    render(<SessionBanner session={withBanner} />);
    expect(screen.getByText("Network changed")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Dismiss message" })).toBeNull();
  });

  it("adds a dismiss button that calls the handler", async () => {
    const onDismiss = vi.fn();
    render(<SessionBanner session={withBanner} onDismiss={onDismiss} />);
    await userEvent.click(screen.getByRole("button", { name: "Dismiss message" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});

describe("StartingNotice", () => {
  it("shows the loading line only while the meeting starts", () => {
    const { rerender } = render(
      <StartingNotice session={{ ...initialSession, status: "starting" }} />,
    );
    expect(screen.getByRole("status").textContent).toContain("Starting meeting");
    for (const status of ["idle", "running", "stopping"] as const) {
      rerender(<StartingNotice session={{ ...initialSession, status }} />);
      expect(screen.queryByRole("status")).toBeNull();
    }
  });
});
