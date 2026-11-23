import type { MeetingSummary, Note } from "../../ipc/types";

/** The most meetings the Today card lists. A starting value chosen for M1-51 (SPEC O-5). */
export const MAX_TODAY_MEETINGS = 5;

/** The local day that holds `nowMs`: from its first millisecond up to, not including, the next day. */
function localDay(nowMs: number): { start: number; end: number } {
  const d = new Date(nowMs);
  return {
    start: new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime(),
    end: new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getTime(),
  };
}

function ofToday<T>(items: readonly T[], nowMs: number, timeOf: (item: T) => number): T[] {
  const { start, end } = localDay(nowMs);
  return items
    .filter((i) => timeOf(i) >= start && timeOf(i) < end)
    .sort((a, b) => timeOf(b) - timeOf(a));
}

/** Meetings that started today, newest first, at most `MAX_TODAY_MEETINGS`. */
export function meetingsOfToday(
  meetings: readonly MeetingSummary[],
  nowMs: number,
): MeetingSummary[] {
  return ofToday(meetings, nowMs, (m) => m.startedAtMs).slice(0, MAX_TODAY_MEETINGS);
}

/** Notes created today, newest first. */
export function notesOfToday(notes: readonly Note[], nowMs: number): Note[] {
  return ofToday(notes, nowMs, (n) => n.createdAtMs);
}
