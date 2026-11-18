import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createClient } from "../ipc/client";
import { ClientProvider } from "../ipc/ClientProvider";
import { createMockTransport } from "../ipc/mockTransport";
import { SessionProvider } from "../ipc/SessionProvider";
import { AppStateProvider } from "./AppState";
import { appReducer, initialAppState } from "./appReducer";
import { NAV_ITEMS, navIdFor, parseSurface } from "./screens";
import { Shell } from "./Shell";
import type { AppState } from "./appReducer";

function renderShell(initial?: Partial<AppState>) {
  return renderToStaticMarkup(
    <ClientProvider client={createClient(createMockTransport(), "mock")}>
      <AppStateProvider initial={initial}>
        <SessionProvider>
          <Shell />
        </SessionProvider>
      </AppStateProvider>
    </ClientProvider>,
  );
}

describe("Shell", () => {
  it("has the five nav entries as buttons and a Settings button", () => {
    const html = renderShell();
    for (const label of ["Home", "Live meeting", "Meetings", "Workspace", "Setup"]) {
      expect(html).toContain(`>${label}</button>`);
    }
    expect(html).toContain(">Settings</button>");
    expect(html).toContain('aria-current="page"');
    expect(NAV_ITEMS.map((n) => n.label)).toEqual([
      "Home",
      "Live meeting",
      "Meetings",
      "Workspace",
      "Setup",
    ]);
  });

  it("starts on Home with its loading state", () => {
    const html = renderShell();
    expect(html).toContain("<h1>Home</h1>");
    expect(html).toContain('role="status"');
  });

  it("does not render the settings sheet until opened", () => {
    expect(renderShell()).not.toContain('role="dialog"');
  });

  it("renders the settings sheet with its five tabs when open", () => {
    const html = renderShell({ settingsOpen: true });
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-label="Close settings"');
    for (const label of ["General", "Voices", "Vocabulary", "Hotkeys", "Models"]) {
      expect(html).toContain(`>${label}</button>`);
    }
  });

  it("shows a toast with role=status", () => {
    const html = renderShell({ toast: { id: 1, message: "Note added.", kind: "info" } });
    expect(html).toContain("Note added.");
  });

  it("shows each screen when navigated to", () => {
    expect(renderShell({ screen: "meetings" })).toContain("<h1>Meetings</h1>");
    expect(renderShell({ screen: "setup" })).toContain("<h1>Setup</h1>");
    expect(renderShell({ screen: "workspace" })).toContain("<h1>Workspace</h1>");
    expect(renderShell({ screen: "live" })).toContain("<h1>Live meeting</h1>");
  });
});

describe("appReducer", () => {
  it("navigates, opens a meeting detail and keeps Meetings highlighted", () => {
    const opened = appReducer(initialAppState, { type: "openMeeting", meetingId: "mtg_1" });
    expect(opened).toMatchObject({ screen: "meeting-detail", meetingId: "mtg_1" });
    expect(navIdFor(opened.screen)).toBe("meetings");
    expect(appReducer(opened, { type: "navigate", screen: "setup" })).toMatchObject({
      screen: "setup",
      meetingId: null,
    });
  });

  it("opens and closes settings, and navigating closes the sheet", () => {
    const open = appReducer(initialAppState, { type: "openSettings" });
    expect(open.settingsOpen).toBe(true);
    expect(appReducer(open, { type: "navigate", screen: "live" }).settingsOpen).toBe(false);
    expect(appReducer(open, { type: "closeSettings" }).settingsOpen).toBe(false);
  });

  it("gives each toast a new id and clears it on dismiss", () => {
    const a = appReducer(initialAppState, { type: "showToast", message: "One" });
    const b = appReducer(a, { type: "showToast", message: "Two", kind: "error" });
    expect(b.toast).toMatchObject({ message: "Two", kind: "error" });
    expect(b.toast?.id).not.toBe(a.toast?.id);
    expect(appReducer(b, { type: "dismissToast" }).toast).toBeNull();
  });
});

describe("parseSurface", () => {
  it("accepts the three surfaces and rejects anything else", () => {
    expect(parseSurface("#surface=subtitle-bar")).toBe("subtitle-bar");
    expect(parseSurface("#surface=quick-actions")).toBe("quick-actions");
    expect(parseSurface("#surface=compact-panel")).toBe("compact-panel");
    expect(parseSurface("#surface=other")).toBeNull();
    expect(parseSurface("")).toBeNull();
  });
});
