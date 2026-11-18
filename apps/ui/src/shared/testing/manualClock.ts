import type { Clock } from "../clock";

export interface ManualClock extends Clock {
  /** Move time forward and tell every subscriber, as a real tick would. */
  advance(ms: number): void;
}

/** A clock that moves only when the test says so. */
export function createManualClock(startMs: number): ManualClock {
  let current = startMs;
  const listeners = new Set<() => void>();
  return {
    now: () => current,
    subscribe(cb) {
      listeners.add(cb);
      return () => {
        listeners.delete(cb);
      };
    },
    advance(ms) {
      current += ms;
      listeners.forEach((cb) => cb());
    },
  };
}
