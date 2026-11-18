// FR-69: the meeting app's microphone must stay on the Interlo virtual microphone.
// The owner is reminded after every full 10 minutes of a running meeting.
export const REMINDER_INTERVAL_MS = 10 * 60 * 1000;

/** How many full reminder intervals have passed since the meeting started. */
export function reminderRound(startedAtMs: number, nowMs: number): number {
  return Math.max(0, Math.floor((nowMs - startedAtMs) / REMINDER_INTERVAL_MS));
}
