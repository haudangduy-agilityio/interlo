import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Transport } from "./transport";

// The two Tauri calls the transport needs. A test passes a fake; the app passes
// `tauriApi`, which wraps the official @tauri-apps/api package. The shell keeps the
// global `window.__TAURI__` off (withGlobalTauri is false), so a page script cannot
// reach the bridge through a global name.
export interface TauriApi {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen(event: string, handler: (e: { payload: unknown }) => void): Promise<() => void>;
}

export const tauriApi: TauriApi = {
  invoke: (cmd, args) => invoke(cmd, args),
  listen: (event, handler) => listen<unknown>(event, handler),
};

declare global {
  interface Window {
    // Injected by the shell in every Tauri webview, with or without the global API.
    __TAURI_INTERNALS__?: unknown;
  }
}

/** True when the page runs inside the Tauri shell. */
export function isTauriShell(): boolean {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;
}

export function createTauriTransport(api: TauriApi): Transport {
  return {
    invoke: (cmd, args) => api.invoke(cmd, args),
    listen(event, cb) {
      // The shell registers listeners asynchronously; the caller needs a sync unsubscribe.
      let unlisten: (() => void) | null = null;
      let cancelled = false;
      api
        .listen(event, (e) => cb(e.payload))
        .then((fn) => {
          if (cancelled) fn();
          else unlisten = fn;
        })
        .catch(() => {
          // Registration failed: the screen stays on its empty state.
        });
      return () => {
        cancelled = true;
        if (unlisten) unlisten();
        unlisten = null;
      };
    },
  };
}
