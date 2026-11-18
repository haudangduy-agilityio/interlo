import type { ReactNode } from "react";
import type { Async } from "../useAsync";
import { ErrorState } from "./ErrorState";
import { LoadingState } from "./LoadingState";

export interface AsyncContentProps<T> {
  state: Async<T>;
  onRetry?: () => void;
  loadingLabel?: string;
  /** When it returns true for loaded data, `empty` is shown instead of `children`. */
  isEmpty?: (data: T) => boolean;
  empty?: ReactNode;
  children: (data: T) => ReactNode;
}

/** The standard loading, error, empty and done switch. */
export function AsyncContent<T>({
  state,
  onRetry,
  loadingLabel,
  isEmpty,
  empty,
  children,
}: AsyncContentProps<T>) {
  if (state.status === "loading") return <LoadingState label={loadingLabel} />;
  if (state.status === "error")
    return <ErrorState message={state.error.message} onRetry={onRetry} />;
  if (isEmpty && isEmpty(state.data)) return <>{empty}</>;
  return <>{children(state.data)}</>;
}
