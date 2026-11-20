// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { createClient, type Client } from "../ipc/client";
import { ClientProvider } from "../ipc/ClientProvider";
import { createMockTransport } from "../ipc/mockTransport";
import type { SetupStatus } from "../ipc/types";
import { SetupStatusProvider, useSetupStatus } from "./SetupStatusProvider";

afterEach(cleanup);

const READY: SetupStatus = { items: [], canStart: true, missingGroups: [] };

function countingClient(results: (SetupStatus | Error)[]): { client: Client; calls: () => number } {
  const base = createClient(createMockTransport({ autoEmit: false }), "mock");
  let n = 0;
  const client: Client = {
    ...base,
    checkSetup: () => {
      const next = results[Math.min(n, results.length - 1)];
      n += 1;
      return next instanceof Error ? Promise.reject(next) : Promise.resolve(next);
    },
  };
  return { client, calls: () => n };
}

// The probe hands the provider API to the test, which calls it inside `act`.
const held: { api: ReturnType<typeof useSetupStatus> | null } = { api: null };
function Probe() {
  const api = useSetupStatus();
  useEffect(() => {
    held.api = api;
  });
  const { state } = api;
  return <p>{state.status === "done" ? `canStart:${state.data.canStart}` : state.status}</p>;
}

function mount(client: Client) {
  render(
    <ClientProvider client={client}>
      <SetupStatusProvider>
        <Probe />
      </SetupStatusProvider>
    </ClientProvider>,
  );
}

describe("SetupStatusProvider (AC-13, AC-35)", () => {
  it("loads once on mount and exposes the result", async () => {
    const { client, calls } = countingClient([READY]);
    mount(client);
    expect(screen.getByText("loading")).toBeTruthy();
    await screen.findByText("canStart:true");
    expect(calls()).toBe(1);
  });

  it("shows the error state when the first check fails, and refresh runs the check again", async () => {
    const { client, calls } = countingClient([new Error("shell crashed"), READY]);
    mount(client);
    await screen.findByText("error");
    let result: SetupStatus | null = null;
    await act(async () => {
      result = (await held.api?.refresh()) ?? null;
    });
    expect(calls()).toBe(2);
    expect(result).toEqual(READY);
    await waitFor(() => expect(screen.getByText("canStart:true")).toBeTruthy());
  });

  it("rejects refresh when the check fails again and keeps the error state", async () => {
    const { client } = countingClient([new Error("first"), new Error("second")]);
    mount(client);
    await screen.findByText("error");
    await act(async () => {
      await expect(held.api?.refresh()).rejects.toMatchObject({ code: "UNKNOWN" });
    });
    expect(screen.getByText("error")).toBeTruthy();
  });

  it("takes a newer result from a screen", async () => {
    const { client } = countingClient([READY]);
    mount(client);
    await screen.findByText("canStart:true");
    act(() => held.api?.publish({ ...READY, canStart: false, missingGroups: ["models"] }));
    expect(screen.getByText("canStart:false")).toBeTruthy();
  });
});
