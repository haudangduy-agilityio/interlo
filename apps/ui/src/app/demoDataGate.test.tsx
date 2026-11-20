// @vitest-environment jsdom
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createClient } from "../ipc/client";
import { createMockTransport } from "../ipc/mockTransport";
import { createManualClock } from "../shared/testing/manualClock";
import { renderApp } from "../shared/testing/renderApp";

// AC-20 (demo_data_gate): the shell and Home show no sample figure, name or sentence of the demo.
// Every number on screen comes from the client (the setup check) or the clock.
const DEMO_SAMPLES = [
  "6.1 / 8 GB",
  "3.1 / 6 GB",
  "76%",
  "52%",
  "Mon 16 Nov",
  "Vendor sync",
  "Design review",
  "Partner 1:1",
  "Send quote to",
  "Fix checkout button",
  "Review release checklist",
  "Run visual tests",
  "asked about the Q3 quote",
  "use 'Build 42'",
  "Change the staging password",
  "42 min",
  "1 h 05",
  "25 min",
];

/** Text and accessible names: visible text, aria-label, title, alt and the hidden hints. */
function everythingRead(): string {
  const parts: string[] = [document.body.textContent ?? ""];
  for (const el of document.body.querySelectorAll("*")) {
    for (const name of ["aria-label", "title", "alt"]) {
      const value = el.getAttribute(name);
      if (value) parts.push(value);
    }
  }
  return parts.join("\n");
}

describe("demo_data_gate (AC-20)", () => {
  it("shows no sample data with empty meetings, empty notes and a setup that is not ready", async () => {
    const clock = createManualClock(new Date(2026, 10, 20, 10, 0).getTime());
    renderApp({ clock, meetings: [], notes: [] });
    await screen.findByText("Some setup items are missing.");
    await screen.findByText("No meetings today");

    const text = everythingRead();
    for (const sample of DEMO_SAMPLES) expect(text, sample).not.toContain(sample);

    // The numbers that do show: the day from the clock and the setup count from the client.
    const status = await createClient(
      createMockTransport({ autoEmit: false }),
      "mock",
    ).checkSetup();
    const missing = status.items.filter((i) => i.state !== "ok").length;
    const visible = (document.body.textContent ?? "").replace(/Ctrl\+Option\+[A-Z]/g, "");
    const numbers = new Set(visible.match(/\d+/g) ?? []);
    expect([...numbers].sort()).toEqual([String(missing), "20"].sort());
  });
});
