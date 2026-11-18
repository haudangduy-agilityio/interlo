import type { StartMeetingArgs } from "../../ipc/types";

/** Start arguments that pass every per-meeting check. Override a field to fail one. */
export function startArgs(overrides: Partial<StartMeetingArgs> = {}): StartMeetingArgs {
  return {
    mode: "both",
    meetingAppId: "browser",
    checks: { headphonesConfirmed: true, meetingMicConfirmed: true },
    ...overrides,
  };
}
