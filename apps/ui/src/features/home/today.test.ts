import { describe, expect, it } from "vitest";
import type { MeetingSummary, Note } from "../../ipc/types";
import { MAX_TODAY_MEETINGS, meetingsOfToday, notesOfToday } from "./today";

// AC-22, AC-23, AC-34. Dates are built from local fields, so the tests hold in any time zone.
const NOW = new Date(2026, 10, 20, 10, 0, 0, 0).getTime();
const at = (day: number, h = 0, m = 0, s = 0, ms = 0) =>
  new Date(2026, 10, day, h, m, s, ms).getTime();

const meeting = (id: string, startedAtMs: number): MeetingSummary => ({
  id,
  title: id,
  startedAtMs,
  durationSec: 600,
  status: "ready",
});
const note = (id: string, createdAtMs: number): Note => ({ id, text: id, tag: null, createdAtMs });

describe("meetingsOfToday (AC-22, AC-34)", () => {
  it("case A: shows the newest five of six meetings of the day, newest first", () => {
    const six = [1, 2, 3, 4, 5, 6].map((h) => meeting(`m${h}`, at(20, h)));
    const shown = meetingsOfToday(six, NOW);
    expect(MAX_TODAY_MEETINGS).toBe(5);
    expect(shown.map((m) => m.id)).toEqual(["m6", "m5", "m4", "m3", "m2"]);
  });

  it("case B: a meeting at 00:00:00.000 counts as today and one at 23:59:59.999 yesterday does not", () => {
    const list = [
      meeting("midnight", at(20, 0, 0, 0, 0)),
      meeting("a", at(20, 1)),
      meeting("b", at(20, 2)),
      meeting("c", at(20, 3)),
      meeting("d", at(20, 4)),
      meeting("yesterday", at(19, 23, 59, 59, 999)),
    ];
    const shown = meetingsOfToday(list, NOW);
    expect(shown.map((m) => m.id)).toEqual(["d", "c", "b", "a", "midnight"]);
    expect(shown.map((m) => m.id)).not.toContain("yesterday");
  });

  it("leaves out a meeting that starts tomorrow", () => {
    expect(meetingsOfToday([meeting("t", at(21, 0, 0, 0, 0))], NOW)).toEqual([]);
  });

  it("gives an empty list for no meetings", () => {
    expect(meetingsOfToday([], NOW)).toEqual([]);
  });
});

describe("notesOfToday (AC-23)", () => {
  it("lists notes created on the local day, newest first, with the edges of the day", () => {
    const notes = [
      note("midnight", at(20, 0, 0, 0, 0)),
      note("noon", at(20, 12)),
      note("yesterday", at(19, 23, 59, 59, 999)),
      note("tomorrow", at(21, 0, 0, 0, 0)),
    ];
    expect(notesOfToday(notes, NOW).map((n) => n.id)).toEqual(["noon", "midnight"]);
  });
});
