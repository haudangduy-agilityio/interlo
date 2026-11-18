import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

/** Time for the screens. Tests pass a manual clock, so they never wait. */
export interface Clock {
  now(): number;
  /** Call `cb` whenever time has moved on. Returns the way to stop. */
  subscribe(cb: () => void): () => void;
}

// Starting value: a reminder that is due at a full minute shows within a second.
const TICK_MS = 1000;

export const systemClock: Clock = {
  now: () => Date.now(),
  subscribe(cb) {
    const timer = window.setInterval(cb, TICK_MS);
    return () => window.clearInterval(timer);
  },
};

const ClockContext = createContext<Clock>(systemClock);

export function ClockProvider({ clock, children }: { clock: Clock; children: ReactNode }) {
  return <ClockContext.Provider value={clock}>{children}</ClockContext.Provider>;
}

export function useClock(): Clock {
  return useContext(ClockContext);
}

/** The current time in ms, updated on each tick of the clock. */
export function useNow(): number {
  const clock = useClock();
  const [now, setNow] = useState(() => clock.now());
  useEffect(() => clock.subscribe(() => setNow(clock.now())), [clock]);
  return now;
}
