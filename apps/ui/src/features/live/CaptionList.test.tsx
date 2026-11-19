import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { CaptionList, formatMs } from "./CaptionList";
import type { Caption } from "../../protocol";

const base: Caption = {
  segmentId: "seg_1",
  sourceId: "incoming",
  startMs: 9200,
  sourceText: "We can ship this on Friday.",
  sourceStatus: "final",
  targetText: "Chúng ta có thể phát hành vào thứ Sáu.",
  targetLanguage: "vi",
  translationStatus: "final",
};

describe("CaptionList", () => {
  it("shows the empty state when there are no captions", () => {
    const html = renderToStaticMarkup(<CaptionList captions={[]} />);
    expect(html).toContain("Waiting for speech.");
  });

  it("renders source and translation side by side", () => {
    const html = renderToStaticMarkup(<CaptionList captions={[base]} />);
    expect(html).toContain("We can ship this on Friday.");
    expect(html).toContain("Chúng ta có thể phát hành vào thứ Sáu.");
    expect(html).not.toContain("badge");
  });

  it("marks partial source and shows a placeholder while translating", () => {
    const html = renderToStaticMarkup(
      <CaptionList
        captions={[
          { ...base, sourceStatus: "partial", targetText: null, translationStatus: "pending" },
        ]}
      />,
    );
    expect(html).toContain("caption--partial");
    expect(html).toContain("translating");
  });

  it("never shows a null target as text", () => {
    const html = renderToStaticMarkup(
      <CaptionList captions={[{ ...base, targetText: null, translationStatus: null }]} />,
    );
    expect(html).toContain('class="muted">…<');
  });
});

describe("formatMs", () => {
  it("formats milliseconds as mm:ss", () => {
    expect(formatMs(9200)).toBe("00:09");
    expect(formatMs(3_725_000)).toBe("62:05");
  });
});
