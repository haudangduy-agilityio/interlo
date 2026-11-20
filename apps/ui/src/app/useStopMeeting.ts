import { useCallback, useEffect, useRef } from "react";
import { toUiError } from "../ipc/errors";
import { useSession } from "../ipc/SessionProvider";
import { useActionFailure, type ActionFailureApi } from "../shared/useActionFailure";

/**
 * Stop the running meeting for the top bar. A failure stays on the page (an error banner with
 * Retry); "no meeting" means it was already over, so nothing is shown.
 */
export function useStopMeeting(): { stop: () => Promise<void>; failure: ActionFailureApi } {
  const { stop: stopSession } = useSession();
  const failure = useActionFailure();
  const { clear, fail } = failure;
  // The banner's Retry runs the latest `stop`, so it needs a way back to it.
  const retry = useRef<() => void>(() => {});
  const stop = useCallback(async () => {
    clear();
    try {
      await stopSession();
    } catch (e) {
      if (toUiError(e).code === "NO_MEETING") return;
      fail(e, () => retry.current());
    }
  }, [stopSession, clear, fail]);
  useEffect(() => {
    retry.current = () => void stop();
  }, [stop]);
  return { stop, failure };
}
