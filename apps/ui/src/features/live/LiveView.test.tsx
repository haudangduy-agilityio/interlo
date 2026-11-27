import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createClient } from "../../ipc/client";
import { initialSession, sessionReducer, type SessionState } from "../../ipc/meetingSession";
import { createMockTransport } from "../../ipc/mockTransport";
import { requestStart } from "../../ipc/startOutcome";
import { startArgs } from "../../shared/testing/startArgs";
import { LiveView, type LiveViewProps } from "./LiveView";

const noop = () => {};

function props(session: SessionState): LiveViewProps {
  return {
    session,
    mode: "both",
    onModeChange: noop,
    onStart: noop,
    onStop: noop,
    onToggleMute: noop,
    onTogglePause: noop,
    onOpenSetup: noop,
    onDismissBanner: noop,
  };
}

const running: SessionState = {
  ...initialSession,
  status: "running",
  meetingId: "m1",
  startedAtMs: 0,
};

describe("LiveView: start meeting refused by setup", () => {
  it("shows the command's message with only Models, and a button to open Setup", async () => {
    const client = createClient(createMockTransport({ autoEmit: false }));
    const outcome = await requestStart(client, startArgs());
    if (outcome.kind === "started" || outcome.kind === "cancelled")
      throw new Error("the mock must refuse to start");
    const session = sessionReducer(initialSession, { type: "startFailed", failure: outcome });
    const html = renderToStaticMarkup(<LiveView {...props(session)} />);
    expect(html).toContain("Finish setup first. Not turned on yet: Models.");
    expect(html).not.toContain("Virtual microphone");
    expect(html).toContain(">Open Setup</button>");
    expect(html).toContain('role="alert"');
  });

  it("failed per-meeting checks are shown by the Before you start panel, not by the notice", () => {
    const session = sessionReducer(initialSession, {
      type: "startFailed",
      failure: {
        kind: "checks-failed",
        message: "Confirm before starting: headphones in use.",
        details: ["headphones"],
      },
    });
    const html = renderToStaticMarkup(<LiveView {...props(session)} />);
    expect(html).not.toContain("Confirm before starting");
    expect(html).not.toContain(">Retry</button>");
    expect(html).toContain(">Start meeting</button>");
  });

  it("a generic start failure shows the message with Retry, not the Setup button", () => {
    const session = sessionReducer(initialSession, {
      type: "startFailed",
      failure: { kind: "error", message: "A meeting is already running." },
    });
    const html = renderToStaticMarkup(<LiveView {...props(session)} />);
    expect(html).toContain("A meeting is already running.");
    expect(html).toContain(">Retry</button>");
    expect(html).not.toContain("Open Setup");
  });
});

describe("LiveView states", () => {
  it("idle with no captions shows the empty captions state and a Start button", () => {
    const html = renderToStaticMarkup(<LiveView {...props(initialSession)} />);
    expect(html).toContain("No captions yet");
    expect(html).toContain(">Start meeting</button>");
  });

  it("starting shows a loading status", () => {
    const html = renderToStaticMarkup(
      <LiveView {...props({ ...initialSession, status: "starting" })} />,
    );
    expect(html).toContain("Starting meeting");
    expect(html).toContain('role="status"');
  });

  it("running with no speech uses CaptionList's waiting text", () => {
    expect(renderToStaticMarkup(<LiveView {...props(running)} />)).toContain("Waiting for speech.");
  });

  it("running shows captions through CaptionList and the latency", () => {
    let session = sessionReducer(running, {
      type: "caption",
      event: {
        lineId: "l1",
        direction: "in",
        isFinal: true,
        source: "We can ship this on Friday.",
        target: "Chúng ta có thể phát hành vào thứ Sáu.",
        delayMs: 450,
      },
      nowMs: 9200,
    });
    session = sessionReducer(session, { type: "latency", event: { p50Ms: 420, p95Ms: 780 } });
    const html = renderToStaticMarkup(<LiveView {...props(session)} />);
    expect(html).toContain("We can ship this on Friday.");
    expect(html).toContain("00:09");
    expect(html).toContain("median 420 ms");
  });

  it("paused is a visible banner on the screen, not only a toast", () => {
    const paused = { ...running, mutedOut: true, mutedIn: true };
    const html = renderToStaticMarkup(<LiveView {...props(paused)} />);
    expect(html).toContain("Paused. Nothing is translated or played until you resume.");
    expect(html).toContain(">Resume</button>");
    expect(renderToStaticMarkup(<LiveView {...props(running)} />)).not.toContain("Paused.");
  });

  it("an event banner from the shell is shown", () => {
    const html = renderToStaticMarkup(
      <LiveView
        {...props({ ...running, banner: { kind: "warning", message: "Network is slow." } })}
      />,
    );
    expect(html).toContain("Network is slow.");
    expect(html).toContain('aria-label="Dismiss message"');
  });
});
