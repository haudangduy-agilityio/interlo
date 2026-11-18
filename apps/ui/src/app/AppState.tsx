import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useReducer,
  type Dispatch,
  type ReactNode,
} from "react";
import type { BannerKind } from "../shared/ui";
import { appReducer, initialAppState, type AppAction, type AppState } from "./appReducer";

interface AppStateApi {
  state: AppState;
  dispatch: Dispatch<AppAction>;
}

const AppStateContext = createContext<AppStateApi | null>(null);

export function AppStateProvider({
  initial,
  children,
}: {
  initial?: Partial<AppState>;
  children: ReactNode;
}) {
  const [state, dispatch] = useReducer(appReducer, { ...initialAppState, ...initial });
  const value = useMemo(() => ({ state, dispatch }), [state]);
  return <AppStateContext.Provider value={value}>{children}</AppStateContext.Provider>;
}

export function useAppState(): AppStateApi {
  const api = useContext(AppStateContext);
  if (!api) throw new Error("useAppState needs an AppStateProvider");
  return api;
}

/** Show a toast. Use it for passing news only; keep lasting states in the screen. */
export function useToast(): (message: string, kind?: BannerKind) => void {
  const { dispatch } = useAppState();
  return useCallback((message, kind) => dispatch({ type: "showToast", message, kind }), [dispatch]);
}
