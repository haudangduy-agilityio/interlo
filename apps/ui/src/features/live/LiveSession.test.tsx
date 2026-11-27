// @vitest-environment jsdom
// The Live screen around a real session: the caption row, a stopped or cancelled start, a
// failed start and the captions of an ended session.
import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { sampleModels } from "../../ipc/mockTransport";
import { renderApp } from "../../shared/testing/renderApp";

const ready = () => ({
  screen: "live" as const,
  models: sampleModels().map((m) => ({ ...m, state: "ok" as const })),
});

const PANEL = "Before you start";
const STARTING = "Starting meeting…";
const FAILED = { code: "START_FAILED", message: "The speech models could not be loaded." } as const;
const NO_MEETING = { code: "NO_MEETING", message: "No meeting is running." } as const;

type App = ReturnType<typeof renderApp>;

const main = () => within(screen.getByRole("main"));
const topBar = () => within(screen.getByRole("toolbar", { name: "Meeting controls" }));
const captionRows = () => screen.queryAllByRole("listitem").filter((li) => li.matches(".caption"));

async function confirmInside(app: App, panel: HTMLElement) {
  const inside = within(panel);
  await app.user.selectOptions(inside.getByRole("combobox", { name: "Meeting app" }), "desktop");
  for (const box of inside.getAllByRole("checkbox")) {
    if (!(box as HTMLInputElement).checked) await app.user.click(box);
  }
  await app.user.click(inside.getByRole("button", { name: "Start now" }));
}

/** Start meeting on the Live screen, then Start now in its panel. */
async function startOnLive(app: App) {
  await app.user.click(await main().findByRole("button", { name: "Start meeting" }));
  await confirmInside(app, await screen.findByRole("region", { name: PANEL }));
}

/** Start meeting in the top bar, then Start now in its dialog. */
async function startInDialog(app: App) {
  await app.user.click(await topBar().findByRole("button", { name: "Start meeting" }));
  const dialog = await screen.findByRole("dialog", { name: PANEL });
  await confirmInside(app, dialog);
  return dialog;
}

/** Holds every start until the test answers it. A stop is counted, and accepted unless
 * `stopError` is given. */
function holdStarts(app: App, stopError?: unknown) {
  const held: { succeed: () => void; fail: (error: unknown) => void }[] = [];
  const stops: string[] = [];
  const real = app.transport.invoke.bind(app.transport);
  vi.spyOn(app.transport, "invoke").mockImplementation((<T,>(
    cmd: string,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    if (cmd === "stop_meeting") {
      stops.push(cmd);
      if (stopError !== undefined) return Promise.reject(stopError);
      return Promise.resolve({ meetingId: "mtg_held" } as T);
    }
    if (cmd !== "start_meeting") return real<T>(cmd, args);
    return new Promise<T>((resolve, reject) =>
      held.push({
        succeed: () => resolve({ meetingId: "mtg_held" } as T),
        fail: reject,
      }),
    );
  }) as typeof app.transport.invoke);
  return { held, stops };
}

/** Answer a held call and let its promise chain finish. */
const answer = (reply: () => void) =>
  act(async () => {
    reply();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

const line = (n: number, source: string) => ({
  lineId: `line-${n}`,
  direction: "out",
  isFinal: true,
  source,
  target: "",
  delayMs: 450,
});

describe("Live screen around a session", () => {
  it("caption_row_shows_delay_and_dash", async () => {
    const app = renderApp(ready());
    app.emit("meeting://status", { state: "running" });
    app.emit("meeting://caption", line(1, "Xin chào mọi người."));

    await screen.findByText("Xin chào mọi người.");
    const [row] = captionRows();
    expect(row.textContent).toContain("450 ms");
    expect(row.querySelector(".caption__tgt")?.textContent).toBe("—");
    expect(row.textContent).not.toContain("translating");
  });

  it.each(["success", "error"])("stopped_start_leaves_the_screen_idle (late %s)", async (late) => {
    const app = renderApp(ready());
    const { held, stops } = holdStarts(app);
    await startOnLive(app);
    expect(await main().findByText(STARTING)).toBeTruthy();

    await app.user.click(main().getByRole("button", { name: "Stop meeting" }));
    await waitFor(() => expect(stops).toHaveLength(1));
    // The panel closes with the stop, so the next start confirms its checks again.
    expect(screen.queryByRole("region", { name: PANEL })).toBeNull();
    await answer(() => (late === "success" ? held[0].succeed() : held[0].fail(FAILED)));

    expect(await main().findByRole("button", { name: "Start meeting" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: PANEL })).toBeNull();
    expect(screen.getByText("State: Idle.")).toBeTruthy();
    expect(screen.queryByText(STARTING)).toBeNull();
    expect(screen.queryByText("Meeting running.")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(FAILED.message)).toBeNull();
  });

  it("stopped_start_leaves_the_screen_idle (a new start is pending)", async () => {
    const app = renderApp(ready());
    const { held } = holdStarts(app);
    await startOnLive(app);
    await app.user.click(await main().findByRole("button", { name: "Stop meeting" }));
    await startOnLive(app);
    await waitFor(() => expect(held).toHaveLength(2));

    await answer(() => held[0].fail(FAILED));
    expect(screen.getByText(STARTING)).toBeTruthy();
    expect(screen.queryByText(FAILED.message)).toBeNull();

    // The new start still owns the panel: its refused checks show there.
    await answer(() =>
      held[1].fail({
        code: "START_CHECK_FAILED",
        message: "Confirm before starting: headphones in use.",
        details: ["headphones"],
      }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Confirm before starting: headphones in use.");
  });

  it("stopped_start_leaves_the_screen_idle (the stop finds no meeting)", async () => {
    const app = renderApp(ready());
    const { stops } = holdStarts(app, NO_MEETING);
    await startOnLive(app);
    await app.user.click(await main().findByRole("button", { name: "Stop meeting" }));
    await waitFor(() => expect(stops).toHaveLength(1));

    expect(await screen.findByText("State: Idle.")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(NO_MEETING.message)).toBeNull();
  });

  it("start_during_a_running_meeting_opens_the_live_screen", async () => {
    const app = renderApp({ ...ready(), screen: "home" });
    const { held, stops } = holdStarts(app);
    app.emit("meeting://status", { state: "running" });
    app.emit("meeting://caption", line(1, "Xin chào mọi người."));
    await app.user.click(await main().findByRole("button", { name: "Start meeting" }));

    expect(await screen.findByText("State: Running.")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByText("Xin chào mọi người.")).toBeTruthy();
    expect(main().getByRole("button", { name: "Stop meeting" })).toBeTruthy();
    expect(held).toEqual([]);
    expect(stops).toEqual([]);
  });

  it("start_during_a_pending_start_opens_the_live_screen", async () => {
    const app = renderApp(ready());
    const { held, stops } = holdStarts(app);
    await startOnLive(app);
    await app.user.click(screen.getByRole("button", { name: "Today" }));
    await app.user.click(await main().findByRole("button", { name: "Start meeting" }));

    expect(await main().findByText(STARTING)).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(held).toHaveLength(1);
    expect(stops).toEqual([]);
  });

  it("cancel_that_cannot_stop_shows_the_failure_on_the_live_screen", async () => {
    const app = renderApp({ ...ready(), screen: "home" });
    holdStarts(app, new Error("the shell did not answer"));
    const dialog = await startInDialog(app);
    await app.user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await app.user.click(screen.getByRole("button", { name: "Live meeting" }));

    const banners = await main().findAllByRole("alert");
    expect(banners.map((banner) => banner.textContent).join()).toContain(
      "the shell did not answer",
    );
    expect(main().getByRole("button", { name: "Stop meeting" })).toBeTruthy();
  });

  it.each(["Cancel", "Close dialog"])(
    "cancel_during_a_pending_start_stops_it (%s)",
    async (control) => {
      const app = renderApp({ ...ready(), screen: "home" });
      const { held, stops } = holdStarts(app);
      const heading = () => main().getAllByRole("heading")[0].textContent;
      const before = await waitFor(heading);
      const dialog = await startInDialog(app);

      await app.user.click(within(dialog).getByRole("button", { name: control }));
      await waitFor(() => expect(stops).toHaveLength(1));
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(heading()).toBe(before);

      await answer(() => held[0].succeed());
      expect(heading()).toBe(before);
      expect(screen.queryByRole("alert")).toBeNull();
      expect(stops).toHaveLength(1);
    },
  );

  it("cancel_during_a_pending_start_stops_it (a late error shows nothing)", async () => {
    const app = renderApp({ ...ready(), screen: "home" });
    const { held } = holdStarts(app);
    const dialog = await startInDialog(app);
    await app.user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await answer(() => held[0].fail(FAILED));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(FAILED.message)).toBeNull();
  });

  it("cancel_during_a_pending_start_stops_it (without a pending start no command)", async () => {
    const app = renderApp({ ...ready(), screen: "home" });
    const { stops } = holdStarts(app);
    for (const control of ["Cancel", "Close dialog"]) {
      await app.user.click(await topBar().findByRole("button", { name: "Start meeting" }));
      const dialog = await screen.findByRole("dialog", { name: PANEL });
      await app.user.click(within(dialog).getByRole("button", { name: control }));
      expect(screen.queryByRole("dialog")).toBeNull();
    }
    expect(stops).toEqual([]);
  });

  it("start_failure_shows_its_message_and_retry (Live screen)", async () => {
    const app = renderApp({ ...ready(), failing: { start_meeting: FAILED } });
    await startOnLive(app);

    expect(await main().findByText(FAILED.message)).toBeTruthy();
    expect(screen.getByText("State: Idle.")).toBeTruthy();
    await app.user.click(main().getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("region", { name: PANEL })).toBeTruthy();
  });

  it("start_failure_shows_its_message_and_retry (Start dialog)", async () => {
    const app = renderApp({ ...ready(), screen: "home", failing: { start_meeting: FAILED } });
    const dialog = await startInDialog(app);

    expect(await within(dialog).findByText(FAILED.message)).toBeTruthy();
    const again = within(dialog).getByRole("button", { name: "Start now" });
    expect((again as HTMLButtonElement).disabled).toBe(false);
  });

  it("captions_stay_after_stop_and_clear_at_start", async () => {
    const app = renderApp(ready());
    await startOnLive(app);
    app.emit("meeting://status", { state: "running" });
    app.emit("meeting://caption", line(1, "Câu đầu tiên."));
    await screen.findByText("Câu đầu tiên.");

    // Ended by Stop.
    await app.user.click(main().getByRole("button", { name: "Stop meeting" }));
    await main().findByRole("button", { name: "Start meeting" });
    expect(screen.getByText("Câu đầu tiên.")).toBeTruthy();

    await startOnLive(app);
    app.emit("meeting://status", { state: "running" });
    expect(screen.queryByText("Câu đầu tiên.")).toBeNull();
    app.emit("meeting://caption", line(2, "Câu thứ hai."));
    await screen.findByText("Câu thứ hai.");

    // Ended by a failure of the shell.
    app.emit("meeting://status", { state: "idle" });
    app.emit("meeting://banner", {
      kind: "error",
      message: "Speech processing stopped. Press Start to try again.",
    });
    expect(screen.getByText("Câu thứ hai.")).toBeTruthy();
    expect(captionRows()).toHaveLength(1);
  });
});
