import type { Client } from "./client";
import { toUiError } from "./errors";
import { START_CHECK_IDS, type StartCheckId, type StartMeetingArgs } from "./types";

export type StartFailure =
  | { kind: "setup-incomplete"; message: string }
  /** Per-meeting checks failed. `details` holds the failed ids, in the contract's order. */
  | { kind: "checks-failed"; message: string; details: StartCheckId[] }
  | { kind: "error"; message: string };

export type StartOutcome =
  | { kind: "started"; meetingId: string }
  /** A stop overtook the start, or a session already existed. There is nothing to show. */
  | { kind: "cancelled" }
  | StartFailure;

function knownChecks(details: readonly string[] | undefined): StartCheckId[] {
  return START_CHECK_IDS.filter((id) => details?.includes(id));
}

/**
 * Ask the shell to start a meeting and sort the result for the screen.
 * SETUP_INCOMPLETE and START_CHECK_FAILED keep the shell's message word for word.
 */
export async function requestStart(client: Client, args: StartMeetingArgs): Promise<StartOutcome> {
  try {
    const { meetingId } = await client.startMeeting(args);
    return { kind: "started", meetingId };
  } catch (e) {
    const error = toUiError(e);
    if (error.code === "SETUP_INCOMPLETE")
      return { kind: "setup-incomplete", message: error.message };
    if (error.code === "START_CHECK_FAILED")
      return { kind: "checks-failed", message: error.message, details: knownChecks(error.details) };
    return { kind: "error", message: error.message };
  }
}
