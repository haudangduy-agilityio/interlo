import { useCallback, useEffect, useState } from "react";
import { toUiError, type UiError } from "../ipc/errors";

export type Async<T> =
  { status: "loading" } | { status: "error"; error: UiError } | { status: "done"; data: T };

export interface AsyncResource<T> {
  state: Async<T>;
  /** Load again. The state goes back to loading. */
  reload(): void;
  /** Replace loaded data without a round trip (for example after a save). */
  setData(update: (data: T) => T): void;
}

/** Run `load` on mount and when `deps` change. A late result from an old run is dropped. */
export function useAsync<T>(load: () => Promise<T>, deps: readonly unknown[]): AsyncResource<T> {
  const [state, setState] = useState<Async<T>>({ status: "loading" });
  const [run, setRun] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading" });
    load().then(
      (data) => {
        if (!cancelled) setState({ status: "done", data });
      },
      (e: unknown) => {
        if (!cancelled) setState({ status: "error", error: toUiError(e) });
      },
    );
    return () => {
      cancelled = true;
    };
    // `load` is meant to follow `deps` only.
  }, [...deps, run]);

  const reload = useCallback(() => setRun((n) => n + 1), []);
  const setData = useCallback((update: (data: T) => T) => {
    setState((s) => (s.status === "done" ? { status: "done", data: update(s.data) } : s));
  }, []);
  return { state, reload, setData };
}

/** Map the data of a done state; loading and error pass through. */
export function mapAsync<T, U>(state: Async<T>, fn: (data: T) => U): Async<U> {
  return state.status === "done" ? { status: "done", data: fn(state.data) } : state;
}
