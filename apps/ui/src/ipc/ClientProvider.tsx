import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { useT } from "../shared/i18n";
import { ErrorState, LoadingState } from "../shared/ui";
import { createDefaultClient, type Client } from "./client";
import { toUiError } from "./errors";

const ClientContext = createContext<Client | null>(null);

type Loaded =
  { status: "loading" } | { status: "error"; message: string } | { status: "done"; client: Client };

/**
 * Provides the IPC client. Pass `client` in tests. Otherwise the default transport is loaded
 * once, and the app waits (or shows why it cannot start) until it is ready.
 */
export function ClientProvider({
  client,
  load = createDefaultClient,
  children,
}: {
  client?: Client;
  load?: () => Promise<Client>;
  children: ReactNode;
}) {
  const t = useT();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });

  useEffect(() => {
    if (client) return;
    let cancelled = false;
    load().then(
      (c) => {
        if (!cancelled) setLoaded({ status: "done", client: c });
      },
      (e: unknown) => {
        if (!cancelled) setLoaded({ status: "error", message: toUiError(e).message });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [client, load]);

  const value = client ?? (loaded.status === "done" ? loaded.client : null);
  if (!value) {
    return loaded.status === "error" ? (
      <ErrorState message={loaded.message} />
    ) : (
      <LoadingState label={t("app.connecting")} />
    );
  }
  return <ClientContext.Provider value={value}>{children}</ClientContext.Provider>;
}

export function useClient(): Client {
  const client = useContext(ClientContext);
  if (!client) throw new Error("useClient needs a ClientProvider");
  return client;
}
