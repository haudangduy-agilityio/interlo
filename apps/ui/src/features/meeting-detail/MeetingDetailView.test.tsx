import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { sampleMeetings } from "../../ipc/mockTransport";
import type { MeetingStatus } from "../../ipc/types";
import { MeetingDetailView } from "./MeetingDetailView";

const noop = () => {};

function renderWith(status: MeetingStatus) {
  const meeting = { ...sampleMeetings()[0], status };
  return renderToStaticMarkup(
    <MeetingDetailView meeting={{ status: "done", data: meeting }} onBack={noop} onRetry={noop} />,
  );
}

describe("MeetingDetailView summary states", () => {
  it("generating shows a status message", () => {
    const html = renderWith("generating");
    expect(html).toContain("Generating summary");
    expect(html).toContain('role="status"');
  });

  it("ready shows the summary regions", () => {
    const html = renderWith("ready");
    expect(html).toContain("Key points");
    expect(html).not.toContain("could not be generated");
  });

  it("failed shows an error with Retry", () => {
    const html = renderWith("failed");
    expect(html).toContain("The summary could not be generated.");
    expect(html).toContain(">Retry</button>");
  });

  it("interrupted warns that the summary may be incomplete", () => {
    expect(renderWith("interrupted")).toContain("may be incomplete");
  });

  it("an unknown meeting id shows 'Meeting not found'", () => {
    const html = renderToStaticMarkup(
      <MeetingDetailView meeting={{ status: "done", data: null }} onBack={noop} onRetry={noop} />,
    );
    expect(html).toContain("Meeting not found");
  });
});
