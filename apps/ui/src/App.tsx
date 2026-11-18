import { useSyncExternalStore } from "react";
import { AppStateProvider, Shell, SurfaceHost, parseSurface } from "./app/index";
import { ErrorBoundary } from "./app/ErrorBoundary";
import { SettingsLanguageSync } from "./app/SettingsLanguageSync";
import { ClientProvider } from "./ipc/ClientProvider";
import { SessionProvider } from "./ipc/SessionProvider";
import { LanguageProvider } from "./shared/i18n";

function subscribeHash(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}
const readHash = () => window.location.hash;
const serverHash = () => "";

/** Entry component. A `#surface=<id>` hash shows one floating surface instead of the shell. */
export function App() {
  const hash = useSyncExternalStore(subscribeHash, readHash, serverHash);
  const surface = parseSurface(hash);
  return (
    <LanguageProvider>
      <ErrorBoundary>
        <ClientProvider>
          <SettingsLanguageSync />
          <AppStateProvider>
            <SessionProvider>
              {surface ? <SurfaceHost surface={surface} /> : <Shell />}
            </SessionProvider>
          </AppStateProvider>
        </ClientProvider>
      </ErrorBoundary>
    </LanguageProvider>
  );
}
