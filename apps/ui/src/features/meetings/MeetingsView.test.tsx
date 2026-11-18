import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { sampleMeetings } from "../../ipc/mockTransport";
import type { Async } from "../../shared/useAsync";
import type { MeetingSummary } from "../../ipc/types";
import { MeetingsView, filterMeetings } from "./MeetingsView";

const noop = () => {};

function render(meetings: Async<MeetingSummary[]>, query = "") {
  return renderToStaticMarkup(
    <MeetingsView
      meetings={meetings}
      query={query}
      onQueryChange={noop}
      onClearSearch={noop}
      onOpen={noop}
      onRetry={noop}
    />,
  );
}

describe("MeetingsView", () => {
  it("shows 'No meetings yet' for an empty library, without a Clear search button", () => {
    const html = render({ status: "done", data: [] });
    expect(html).toContain("No meetings yet");
    expect(html).not.toContain("No meetings match");
    expect(html).not.toContain("Clear search");
  });

  it("shows 'No meetings match' and a Clear search button for a search miss", () => {
    const html = render({ status: "done", data: sampleMeetings() }, "zzz-no-such-title");
    expect(html).toContain("No meetings match");
    expect(html).toContain(">Clear search</button>");
    expect(html).not.toContain("No meetings yet");
  });

  it("lists matching meetings with their status", () => {
    const html = render({ status: "done", data: sampleMeetings() }, "weekly");
    expect(html).toContain("Weekly planning");
    expect(html).not.toContain("Vendor call");
    expect(html).toContain("Summary ready");
  });

  it("shows loading with role=status and an error with Retry", () => {
    expect(render({ status: "loading" })).toContain('role="status"');
    const error = render({
      status: "error",
      error: { code: "UNKNOWN", message: "Could not load meetings." },
    });
    expect(error).toContain("Could not load meetings.");
    expect(error).toContain(">Retry</button>");
  });
});

describe("filterMeetings", () => {
  it("ignores case and surrounding spaces, and returns all for an empty query", () => {
    const all = sampleMeetings();
    expect(filterMeetings(all, "  VENDOR ")).toHaveLength(1);
    expect(filterMeetings(all, "")).toHaveLength(all.length);
  });
});
