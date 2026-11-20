// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MeetingSummary, Note } from "../../ipc/types";
import type { Async } from "../../shared/useAsync";
import { HomeView, type HomeViewProps } from "./HomeView";

afterEach(cleanup);

// Friday 20 November 2026, 10:00 local time. Built from local fields, so any time zone works.
const NOW = new Date(2026, 10, 20, 10, 0).getTime();
const at = (h: number, m = 0) => new Date(2026, 10, 20, h, m).getTime();
const meeting = (id: string, title: string, startedAtMs: number): MeetingSummary => ({
  id,
  title,
  startedAtMs,
  durationSec: 1800,
  status: "ready",
});
const note = (id: string, text: string, tag: string | null, createdAtMs: number): Note => ({
  id,
  text,
  tag,
  createdAtMs,
});
const done = <T,>(data: T): Async<T> => ({ status: "done", data });

function props(overrides: Partial<HomeViewProps> = {}): HomeViewProps {
  return {
    nowMs: NOW,
    meetings: done([]),
    notes: done([]),
    onRetryMeetings: vi.fn(),
    onRetryNotes: vi.fn(),
    onStartMeeting: vi.fn(),
    onOpenMeeting: vi.fn(),
    onSeeAllMeetings: vi.fn(),
    onOpenWorkspace: vi.fn(),
    ...overrides,
  };
}

// AC-21: the structure of the Today view.
describe("Home structure (AC-21, AC-24)", () => {
  it("has the Today heading, the date, and the three quick actions with their keys, all disabled", () => {
    render(<HomeView {...props()} />);
    expect(screen.getByRole("heading", { name: "Today", level: 1 })).toBeTruthy();
    expect(
      screen.getByText(/^Friday, November 20 · tasks, notes and meetings today$/),
    ).toBeTruthy();
    const keys = new Map([
      ["Quick translate", "Ctrl+Option+T"],
      ["Fix grammar", "Ctrl+Option+G"],
      ["Save note", "Ctrl+Option+N"],
    ]);
    for (const [name, combo] of keys) {
      const button = screen.getByRole("button", { name: new RegExp(`^${name}`) });
      expect((button as HTMLButtonElement).disabled).toBe(true);
      expect(button.textContent).toContain(combo);
      const hint = button.getAttribute("aria-describedby") ?? "";
      expect(document.getElementById(hint)?.textContent).toBe("Not available yet");
    }
  });

  it("shows the cards Tasks, Today's notes, Today's meetings and Floating panels in a grid", () => {
    const { container } = render(<HomeView {...props()} />);
    const grid = container.querySelector(".grid.grid--2") as HTMLElement;
    const titles = [...grid.querySelectorAll("h2")].map((h) => h.textContent);
    expect(titles).toEqual([
      "Tasks",
      "Today's notes",
      "Today's meetings",
      "Floating panels (preview)",
    ]);
  });

  it("shows the Tasks card as not available, with no count and no row", () => {
    render(<HomeView {...props()} />);
    const tasks = screen.getByRole("region", { name: "Tasks" });
    expect(within(tasks).getByText("Not available yet")).toBeTruthy();
    expect(tasks.querySelector(".rowi")).toBeNull();
    expect(tasks.querySelector(".chip")).toBeNull();
  });
});

describe("Today's meetings (AC-22)", () => {
  const card = () => screen.getByRole("region", { name: "Today's meetings" });

  it("loading shows the loading text", () => {
    render(<HomeView {...props({ meetings: { status: "loading" } })} />);
    expect(within(card()).getByText("Loading meetings…")).toBeTruthy();
  });

  it("empty shows the title and what to do", () => {
    render(<HomeView {...props()} />);
    expect(within(card()).getByText("No meetings today")).toBeTruthy();
    expect(within(card()).getByText("Press Start meeting to begin one.")).toBeTruthy();
  });

  it("error shows the message and Retry calls the load again", async () => {
    const onRetryMeetings = vi.fn();
    render(
      <HomeView
        {...props({
          meetings: { status: "error", error: { code: "UNKNOWN", message: "Could not load." } },
          onRetryMeetings,
        })}
      />,
    );
    expect(within(card()).getByText("Could not load.")).toBeTruthy();
    await userEvent.click(within(card()).getByRole("button", { name: "Retry" }));
    expect(onRetryMeetings).toHaveBeenCalledTimes(1);
  });

  it("done lists today's meetings with title, time, length and Open, and leaves out other days", async () => {
    const onOpenMeeting = vi.fn();
    const yesterday = new Date(2026, 10, 19, 23, 59, 59, 999).getTime();
    render(
      <HomeView
        {...props({
          meetings: done([
            meeting("a", "Standup", at(9)),
            meeting("b", "Planning", at(8, 30)),
            meeting("c", "Old call", yesterday),
          ]),
          onOpenMeeting,
        })}
      />,
    );
    const rows = within(card()).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("Standup");
    expect(rows[0].textContent).toContain("30m 00s");
    expect(within(card()).queryByText("Old call")).toBeNull();
    await userEvent.click(within(card()).getByRole("button", { name: "Open Standup" }));
    expect(onOpenMeeting).toHaveBeenCalledWith("a");
  });
});

describe("Today's notes (AC-23)", () => {
  const card = () => screen.getByRole("region", { name: "Today's notes" });

  it("loading shows the loading text", () => {
    render(<HomeView {...props({ notes: { status: "loading" } })} />);
    expect(within(card()).getByText("Loading notes…")).toBeTruthy();
  });

  it("empty shows the title and what to do", () => {
    render(<HomeView {...props()} />);
    expect(within(card()).getByText("No notes today")).toBeTruthy();
    expect(within(card()).getByText("Save a note and it shows here.")).toBeTruthy();
  });

  it("error shows the message and Retry calls the load again", async () => {
    const onRetryNotes = vi.fn();
    render(
      <HomeView
        {...props({
          notes: { status: "error", error: { code: "UNKNOWN", message: "Notes failed." } },
          onRetryNotes,
        })}
      />,
    );
    expect(within(card()).getByText("Notes failed.")).toBeTruthy();
    await userEvent.click(within(card()).getByRole("button", { name: "Retry" }));
    expect(onRetryNotes).toHaveBeenCalledTimes(1);
  });

  it("done lists today's notes with text, tag chip and time, and Open Workspace works", async () => {
    const onOpenWorkspace = vi.fn();
    render(
      <HomeView
        {...props({
          notes: done([
            note("n1", "Ask about the invoice", "Acme", at(9, 15)),
            note("n2", "Yesterday's idea", null, new Date(2026, 10, 19, 23, 59, 59, 999).getTime()),
          ]),
          onOpenWorkspace,
        })}
      />,
    );
    const rows = within(card()).getAllByRole("listitem");
    expect(rows).toHaveLength(1);
    expect(rows[0].textContent).toContain("Ask about the invoice");
    expect(rows[0].querySelector(".chip")?.textContent).toBe("Acme");
    await userEvent.click(within(card()).getByRole("button", { name: "Open Workspace" }));
    expect(onOpenWorkspace).toHaveBeenCalledTimes(1);
  });
});

// AC-25: what the old Home offered still has a path.
describe("nothing lost from Home (AC-25)", () => {
  it("Start meeting and See all are on the Today's meetings card", async () => {
    const onStartMeeting = vi.fn();
    const onSeeAllMeetings = vi.fn();
    render(<HomeView {...props({ onStartMeeting, onSeeAllMeetings })} />);
    const card = screen.getByRole("region", { name: "Today's meetings" });
    await userEvent.click(within(card).getByRole("button", { name: "Start meeting" }));
    await userEvent.click(within(card).getByRole("button", { name: "See all" }));
    expect(onStartMeeting).toHaveBeenCalledTimes(1);
    expect(onSeeAllMeetings).toHaveBeenCalledTimes(1);
  });

  it("the subtitle bar and quick actions panels are still linked", () => {
    render(<HomeView {...props()} />);
    const card = screen.getByRole("region", { name: "Floating panels (preview)" });
    expect(within(card).getByRole("link", { name: "Subtitle bar" }).getAttribute("href")).toBe(
      "#surface=subtitle-bar",
    );
    expect(within(card).getByRole("link", { name: "Quick actions" }).getAttribute("href")).toBe(
      "#surface=quick-actions",
    );
  });
});
