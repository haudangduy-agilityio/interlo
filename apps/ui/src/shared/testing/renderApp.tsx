import { act, cleanup, render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach } from "vitest";
import { AppStateProvider, Shell, SurfaceHost, type SurfaceId } from "../../app/index";
import { SettingsLanguageSync } from "../../app/SettingsLanguageSync";
import type { AppState } from "../../app/appReducer";
import { createClient } from "../../ipc/client";
import { ClientProvider } from "../../ipc/ClientProvider";
import { createMockTransport, type MockOptions, type MockTransport } from "../../ipc/mockTransport";
import { SessionProvider } from "../../ipc/SessionProvider";
import { ClockProvider, systemClock, type Clock } from "../clock";
import { LanguageProvider } from "../i18n";
import type { Transport } from "../../ipc/transport";
import type { IpcError } from "../../ipc/types";

// Vitest globals are off, so Testing Library does not clean up on its own.
afterEach(cleanup);

export interface FlakyTransport extends MockTransport {
  /**
   * Make every later call of `cmd` reject until `heal(cmd)`. A string rejects like a shell
   * crash (code UNKNOWN in the UI); an IpcError rejects with its contract code.
   */
  failCommand(cmd: string, failure: string | IpcError): void;
  heal(cmd: string): void;
}

/** The mock transport plus a switch to make single commands reject. */
export function createFlakyTransport(options: MockOptions = {}): FlakyTransport {
  const mock = createMockTransport({ autoEmit: false, ...options });
  const failures = new Map<string, string | IpcError>();
  const transport: Transport = {
    invoke: <T,>(cmd: string, args?: Record<string, unknown>) => {
      const failure = failures.get(cmd);
      if (failure === undefined) return mock.invoke<T>(cmd, args);
      return Promise.reject(typeof failure === "string" ? new Error(failure) : failure);
    },
    listen: mock.listen,
  };
  return {
    ...transport,
    emit: mock.emit,
    failCommand: (cmd, failure) => void failures.set(cmd, failure),
    heal: (cmd) => void failures.delete(cmd),
  };
}

export interface RenderAppOptions extends MockOptions {
  /** The screen the shell opens on. Default: home. */
  screen?: AppState["screen"];
  /** Render one floating surface instead of the shell. */
  surface?: SurfaceId;
  /** Commands that reject from the first call, as `{ command: message or IpcError }`. */
  failing?: Record<string, string | IpcError>;
  /** The clock the screens read. Default: the system clock. */
  clock?: Clock;
}

/** Renders the whole shell on the mock transport. Events are off unless the test pushes them. */
export function renderApp({
  screen,
  surface,
  failing = {},
  clock = systemClock,
  ...mockOptions
}: RenderAppOptions = {}) {
  const transport = createFlakyTransport(mockOptions);
  for (const [cmd, message] of Object.entries(failing)) transport.failCommand(cmd, message);
  const client = createClient(transport, "mock");
  const user = userEvent.setup();
  render(
    <ClockProvider clock={clock}>
      <LanguageProvider>
        <ClientProvider client={client}>
          <SettingsLanguageSync />
          <AppStateProvider initial={screen ? { screen } : undefined}>
            <SessionProvider>
              {surface ? <SurfaceHost surface={surface} /> : <Shell />}
            </SessionProvider>
          </AppStateProvider>
        </ClientProvider>
      </LanguageProvider>
    </ClockProvider>,
  );
  return {
    user,
    transport,
    /** Push a shell event inside act, as the shell would. */
    emit: (event: string, payload: unknown) => act(() => transport.emit(event, payload)),
  };
}
