// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createClient, type Client } from "./client";
import { ClientProvider, useClient } from "./ClientProvider";
import { createMockTransport } from "./mockTransport";

afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
});

function Kind() {
  return <p>client:{useClient().kind}</p>;
}

describe("ClientProvider", () => {
  it("shows a loading status, then the app, while the default client loads", async () => {
    render(
      <ClientProvider>
        <Kind />
      </ClientProvider>,
    );
    expect(screen.getByRole("status")).toBeTruthy();
    expect(await screen.findByText("client:mock")).toBeTruthy();
  });

  it("shows an alert with the reason when no client can be made", async () => {
    const load = (): Promise<Client> =>
      Promise.reject(new Error("The desktop shell is not connected."));
    render(
      <ClientProvider load={load}>
        <Kind />
      </ClientProvider>,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("The desktop shell is not connected.");
    expect(screen.queryByText(/^client:/)).toBeNull();
  });

  it("uses a client passed in without loading", () => {
    render(
      <ClientProvider client={createClient(createMockTransport(), "mock")}>
        <Kind />
      </ClientProvider>,
    );
    expect(screen.getByText("client:mock")).toBeTruthy();
  });

  it("shows the alert and never the app in a production build without the desktop shell", async () => {
    // The real default loader runs: no Tauri global and no development flag.
    vi.stubEnv("DEV", false);
    render(
      <ClientProvider>
        <Kind />
      </ClientProvider>,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("The desktop shell is not connected.");
    expect(screen.queryByText(/^client:/)).toBeNull();
  });
});
