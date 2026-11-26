import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  type ReactNode,
} from "react";
import { useClock } from "../shared/clock";
import { useClient } from "./ClientProvider";
import { toUiError } from "./errors";
import { initialSession, sessionReducer, type SessionState } from "./meetingSession";
import {
  initialModelsProgress,
  modelsProgressReducer,
  type ModelsProgressState,
} from "./modelsProgress";
import { requestStart, type StartOutcome } from "./startOutcome";
import type { CaptionEvent, Direction, StartMeetingArgs } from "./types";

export interface SessionApi {
  session: SessionState;
  /** The latest `models://progress` event of each model. */
  models: ModelsProgressState;
  start(args: StartMeetingArgs): Promise<StartOutcome>;
  stop(): Promise<void>;
  setMute(direction: Direction, muted: boolean): Promise<void>;
  /** Pause mutes both directions; resume unmutes them. */
  setPaused(paused: boolean): Promise<void>;
  dismissBanner(): void;
  /** Forget what is known about one model download, for example after Cancel or Dismiss. */
  clearModelProgress(id: string): void;
  /** Forget the last failed start, for example once the cause is fixed. */
  clearStartFailure(): void;
}

// Starting value: well above the events one frame can carry, well below a memory problem.
const MAX_BUFFERED_CAPTIONS = 200;

const SessionContext = createContext<SessionApi | null>(null);

export function SessionProvider({
  initial,
  children,
}: {
  initial?: SessionState;
  children: ReactNode;
}) {
  const client = useClient();
  const clock = useClock();
  const [session, dispatch] = useReducer(sessionReducer, initial ?? initialSession);
  const [models, dispatchModels] = useReducer(modelsProgressReducer, initialModelsProgress);
  // Caption events can arrive many times per frame. They wait here and reach the reducer
  // once per animation frame, so the screen renders once per frame, not once per event.
  const buffered = useRef<{ event: CaptionEvent; nowMs: number }[]>([]);
  const frame = useRef<number | null>(null);
  // Counts the starts and the stops, so the answer of an overtaken start is known.
  const attempt = useRef(0);

  const flush = useCallback(() => {
    frame.current = null;
    if (buffered.current.length === 0) return;
    const items = buffered.current;
    buffered.current = [];
    dispatch({ type: "captions", items });
  }, []);

  useEffect(() => {
    const offs = [
      // A status change applies after every caption that arrived before it.
      client.onStatus((e) => {
        flush();
        dispatch({ type: "status", state: e.state });
      }),
      client.onCaption((e) => {
        buffered.current.push({ event: e, nowMs: clock.now() });
        // A hidden window gets no animation frames. Do not let the buffer grow while it waits.
        if (buffered.current.length >= MAX_BUFFERED_CAPTIONS) flush();
        else frame.current ??= requestAnimationFrame(flush);
      }),
      client.onLatency((e) => dispatch({ type: "latency", event: e })),
      client.onBanner((e) => dispatch({ type: "banner", event: e })),
      client.onModelsProgress((e) => dispatchModels({ type: "progress", event: e })),
    ];
    return () => {
      offs.forEach((off) => off());
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
      buffered.current = [];
    };
  }, [client, clock, flush]);

  const start = useCallback(
    async (args: StartMeetingArgs) => {
      // A new meeting starts with no lines, so captions still waiting from the last one are dropped.
      buffered.current = [];
      dispatch({ type: "startRequested" });
      const mine = (attempt.current += 1);
      const outcome = await requestStart(client, args);
      // A stop or a newer start came first. This answer stays open for good, so no screen
      // acts on it.
      if (mine !== attempt.current) return new Promise<StartOutcome>(() => {});
      if (outcome.kind === "started")
        dispatch({ type: "started", meetingId: outcome.meetingId, nowMs: clock.now() });
      else dispatch({ type: "startFailed", failure: outcome });
      return outcome;
    },
    [client, clock],
  );

  const stop = useCallback(async () => {
    attempt.current += 1;
    flush();
    dispatch({ type: "status", state: "stopping" });
    try {
      await client.stopMeeting();
      dispatch({ type: "status", state: "idle" });
    } catch (e) {
      const error = toUiError(e);
      dispatch({ type: "status", state: error.code === "NO_MEETING" ? "idle" : "running" });
      throw error;
    }
  }, [client, flush]);

  const setMute = useCallback(
    async (direction: Direction, muted: boolean) => {
      await client.setMute({ direction, muted });
      dispatch({ type: "muted", direction, muted });
    },
    [client],
  );

  const setPaused = useCallback(
    async (paused: boolean) => {
      await setMute("out", paused);
      await setMute("in", paused);
    },
    [setMute],
  );

  const dismissBanner = useCallback(() => dispatch({ type: "dismissBanner" }), []);
  const clearModelProgress = useCallback(
    (id: string) => dispatchModels({ type: "cleared", id }),
    [],
  );
  const clearStartFailure = useCallback(() => dispatch({ type: "startFailureCleared" }), []);

  const value = useMemo<SessionApi>(
    () => ({
      session,
      models,
      start,
      stop,
      setMute,
      setPaused,
      dismissBanner,
      clearModelProgress,
      clearStartFailure,
    }),
    [
      session,
      models,
      start,
      stop,
      setMute,
      setPaused,
      dismissBanner,
      clearModelProgress,
      clearStartFailure,
    ],
  );
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): SessionApi {
  const api = useContext(SessionContext);
  if (!api) throw new Error("useSession needs a SessionProvider");
  return api;
}
