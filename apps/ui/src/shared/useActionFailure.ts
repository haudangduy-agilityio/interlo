import { useCallback, useMemo, useState } from "react";
import { toUiError } from "../ipc/errors";
import type { ActionErrorBannerProps } from "./ui";

interface ActionFailure {
  message: string;
  retry?: () => void;
}

export interface ActionFailureApi {
  /** Props for ActionErrorBanner, or undefined while nothing has failed. */
  banner: ActionErrorBannerProps | undefined;
  /** Record a rejection. Pass `retry` when running the same action again makes sense. */
  fail(error: unknown, retry?: () => void): void;
  clear(): void;
}

/** Keeps the last failed action of a screen, so the page can show it next to the toast. */
export function useActionFailure(): ActionFailureApi {
  const [failure, setFailure] = useState<ActionFailure | null>(null);
  const fail = useCallback((error: unknown, retry?: () => void) => {
    setFailure({ message: toUiError(error).message, retry });
  }, []);
  const clear = useCallback(() => setFailure(null), []);
  const banner = useMemo<ActionErrorBannerProps | undefined>(
    () =>
      failure ? { message: failure.message, onRetry: failure.retry, onDismiss: clear } : undefined,
    [failure, clear],
  );
  return { banner, fail, clear };
}
