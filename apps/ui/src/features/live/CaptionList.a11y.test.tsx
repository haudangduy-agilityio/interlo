// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CaptionList } from "./CaptionList";
import type { Caption } from "../../protocol";

afterEach(cleanup);

function line(segmentId: string, sourceId: string, text = segmentId): Caption {
  return {
    segmentId,
    sourceId,
    startMs: 0,
    sourceText: text,
    sourceStatus: "final",
    targetText: null,
    targetLanguage: null,
    translationStatus: "pending",
  };
}

/** jsdom has no layout, so a test sets the scroll metrics by hand. */
function fakeLayout(log: HTMLElement, metrics: { scrollHeight: number; clientHeight: number }) {
  Object.defineProperty(log, "scrollHeight", { configurable: true, value: metrics.scrollHeight });
  Object.defineProperty(log, "clientHeight", { configurable: true, value: metrics.clientHeight });
}

describe("CaptionList people labels", () => {
  it("shows Partner and You, never the internal ids", () => {
    render(<CaptionList captions={[line("a", "incoming"), line("b", "outgoing")]} />);
    expect(screen.getByText(/· Partner$/)).toBeTruthy();
    expect(screen.getByText(/· You$/)).toBeTruthy();
    expect(screen.queryByText(/incoming|outgoing/)).toBeNull();
  });

  it("shows another source id as it is", () => {
    render(<CaptionList captions={[line("a", "room-mic")]} />);
    expect(screen.getByText(/· room-mic$/)).toBeTruthy();
  });
});

describe("CaptionList log region", () => {
  it("is a named log that a screen reader does not announce on every update", () => {
    render(<CaptionList captions={[line("a", "incoming")]} />);
    const log = screen.getByRole("log", { name: "Captions" });
    expect(log.getAttribute("aria-live")).toBe("off");
    // Focusable, so the arrow keys scroll it.
    expect(log.tabIndex).toBe(0);
  });

  it("scrolls to the newest line while the reader is at the end", () => {
    const { rerender } = render(<CaptionList captions={[line("a", "incoming")]} />);
    const log = screen.getByRole("log");
    fakeLayout(log, { scrollHeight: 500, clientHeight: 200 });
    rerender(<CaptionList captions={[line("a", "incoming"), line("b", "incoming")]} />);
    expect(log.scrollTop).toBe(500);
    expect(screen.queryByRole("button", { name: "Latest" })).toBeNull();
  });

  it("stays where it is after the reader scrolls up, and offers a Latest button", async () => {
    const { rerender } = render(<CaptionList captions={[line("a", "incoming")]} />);
    const log = screen.getByRole("log");
    fakeLayout(log, { scrollHeight: 500, clientHeight: 200 });
    log.scrollTop = 100;
    fireEvent.scroll(log);
    expect(screen.getByRole("button", { name: "Latest" })).toBeTruthy();

    fakeLayout(log, { scrollHeight: 700, clientHeight: 200 });
    rerender(<CaptionList captions={[line("a", "incoming"), line("b", "incoming")]} />);
    expect(log.scrollTop).toBe(100);

    act(() => screen.getByRole("button", { name: "Latest" }).click());
    expect(log.scrollTop).toBe(700);
    expect(screen.queryByRole("button", { name: "Latest" })).toBeNull();
  });

  it("counts a reader within the tolerance of the end as being at the end", () => {
    render(<CaptionList captions={[line("a", "incoming")]} />);
    const log = screen.getByRole("log");
    fakeLayout(log, { scrollHeight: 500, clientHeight: 200 });
    log.scrollTop = 300 - 24;
    fireEvent.scroll(log);
    expect(screen.queryByRole("button", { name: "Latest" })).toBeNull();
    log.scrollTop = 300 - 25;
    fireEvent.scroll(log);
    expect(screen.getByRole("button", { name: "Latest" })).toBeTruthy();
  });

  it("follows the newest line again once the reader scrolls back to the end", () => {
    const { rerender } = render(<CaptionList captions={[line("a", "incoming")]} />);
    const log = screen.getByRole("log");
    fakeLayout(log, { scrollHeight: 500, clientHeight: 200 });
    log.scrollTop = 0;
    fireEvent.scroll(log);
    log.scrollTop = 300;
    fireEvent.scroll(log);
    fakeLayout(log, { scrollHeight: 800, clientHeight: 200 });
    rerender(<CaptionList captions={[line("a", "incoming"), line("b", "incoming")]} />);
    expect(log.scrollTop).toBe(800);
  });
});
