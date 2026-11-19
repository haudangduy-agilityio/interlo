// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CaptionList } from "./CaptionList";
import type { Caption } from "../../protocol";

afterEach(cleanup);

function line(segmentId: string, text: string): Caption {
  return {
    segmentId,
    sourceId: "incoming",
    startMs: 0,
    sourceText: text,
    sourceStatus: "final",
    targetText: null,
    targetLanguage: null,
    translationStatus: "pending",
  };
}

/** A caption that counts how often a row reads its source text, which happens once per row render. */
function counted(segmentId: string, text: string): { caption: Caption; renders: () => number } {
  let reads = 0;
  const caption = line(segmentId, text);
  Object.defineProperty(caption, "sourceText", {
    enumerable: true,
    get() {
      reads += 1;
      return text;
    },
  });
  return { caption, renders: () => reads };
}

describe("CaptionList rows", () => {
  it("does not render an unchanged row again when another row changes", () => {
    const first = counted("l1", "First line");
    const second = line("l2", "Second line");
    const { rerender } = render(<CaptionList captions={[first.caption, second]} />);
    const afterMount = first.renders();
    expect(afterMount).toBeGreaterThan(0);

    // A new array with the same first caption object and a changed second line.
    rerender(<CaptionList captions={[first.caption, { ...second, sourceText: "Second line!" }]} />);
    expect(first.renders()).toBe(afterMount);
  });

  it("shows the new text when the caption of a row changes", () => {
    const { rerender, getByText } = render(<CaptionList captions={[line("l1", "First line")]} />);
    rerender(<CaptionList captions={[line("l1", "First line (final)")]} />);
    // getByText throws when the text is missing, so it needs no assertion around it.
    getByText("First line (final)");
  });
});
