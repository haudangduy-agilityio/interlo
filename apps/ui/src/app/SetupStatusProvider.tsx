import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useClient } from "../ipc/ClientProvider";
import { toUiError } from "../ipc/errors";
import type { SetupStatus } from "../ipc/types";
import type { Async } from "../shared/useAsync";

interface SetupStatusApi {
  /** The last setup check: the sidebar badge, the Local-only card and Start read it. */
  state: Async<SetupStatus>;
  /** Check again now and return the result. A failed check also sets the error state. */
  refresh: () => Promise<SetupStatus>;
  /** A screen that already holds a newer result passes it here. */
  publish: (status: SetupStatus) => void;
}

const SetupStatusContext = createContext<SetupStatusApi | null>(null);

/** One `checkSetup` load for everything in the shell that needs the readiness of the setup. */
export function SetupStatusProvider({ children }: { children: ReactNode }) {
  const client = useClient();
  const [state, setState] = useState<Async<SetupStatus>>({ status: "loading" });

  const refresh = useCallback(async () => {
    try {
      const data = await client.checkSetup();
      setState({ status: "done", data });
      return data;
    } catch (e) {
      const error = toUiError(e);
      setState({ status: "error", error });
      throw error;
    }
  }, [client]);

  // The first check runs here; a later one runs through `refresh`.
  useEffect(() => {
    let cancelled = false;
    client.checkSetup().then(
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
  }, [client]);

  const publish = useCallback((data: SetupStatus) => setState({ status: "done", data }), []);
  const value = useMemo(() => ({ state, refresh, publish }), [state, refresh, publish]);
  return <SetupStatusContext.Provider value={value}>{children}</SetupStatusContext.Provider>;
}

export function useSetupStatus(): SetupStatusApi {
  const api = useContext(SetupStatusContext);
  if (!api) throw new Error("useSetupStatus needs a SetupStatusProvider");
  return api;
}
