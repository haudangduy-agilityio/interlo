import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { RUN_STATE_KEY } from "../../ipc/labels";
import { initialSession, type SessionState } from "../../ipc/meetingSession";
import { MEETING_RUN_STATES } from "../../ipc/types";
import { CompactPanelView } from "../compact-panel/CompactPanelView";
import { LiveView } from "./LiveView";

const noop = () => {};
const liveProps = (session: SessionState) => ({
  session,
  mode: "both" as const,
  onModeChange: noop,
  onStart: noop,
  onStop: noop,
  onToggleMute: noop,
  onTogglePause: noop,
  onOpenSetup: noop,
  onDismissBanner: noop,
});

const LABELS = { idle: "Idle", starting: "Starting", running: "Running", stopping: "Stopping" };

describe("meeting state labels", () => {
  it("every state has a label key", () => {
    expect(Object.keys(RUN_STATE_KEY).sort()).toEqual([...MEETING_RUN_STATES].sort());
  });

  it.each(MEETING_RUN_STATES)("the live screen shows %s as a people label", (status) => {
    const html = renderToStaticMarkup(<LiveView {...liveProps({ ...initialSession, status })} />);
    expect(html).toContain(`State: ${LABELS[status]}.`);
    expect(html).not.toContain(`State: ${status}.`);
  });

  it("the compact panel shows the same label, not the raw state", () => {
    const html = renderToStaticMarkup(
      <CompactPanelView
        session={{ ...initialSession, status: "running" }}
        onToggleMute={noop}
        onTogglePause={noop}
        onStop={noop}
      />,
    );
    expect(html).toContain("State: Running.");
    expect(html).not.toContain("State: running.");
  });
});
