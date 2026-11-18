import { useCallback, useEffect } from "react";
import { HomeScreen } from "../features/home";
import { LiveScreen } from "../features/live";
import { MeetingDetailScreen } from "../features/meeting-detail";
import { MeetingsScreen } from "../features/meetings";
import { SetupScreen } from "../features/setup";
import { WorkspaceScreen } from "../features/workspace";
import { useClient } from "../ipc/ClientProvider";
import { useT } from "../shared/i18n";
import { Banner, Button, Toast } from "../shared/ui";
import { useAppState } from "./AppState";
import type { ScreenId } from "./screens";
import { NAV_ITEMS, navIdFor } from "./screens";
import { SettingsSheet } from "./SettingsSheet";

const TOAST_MS = 4000;

function ScreenOutlet({ screen, meetingId }: { screen: ScreenId; meetingId: string | null }) {
  switch (screen) {
    case "home":
      return <HomeScreen />;
    case "live":
      return <LiveScreen />;
    case "meetings":
      return <MeetingsScreen />;
    case "meeting-detail":
      return <MeetingDetailScreen meetingId={meetingId} />;
    case "workspace":
      return <WorkspaceScreen />;
    case "setup":
      return <SetupScreen />;
  }
}

export function Shell() {
  const { state, dispatch } = useAppState();
  const client = useClient();
  const t = useT();
  const activeNav = navIdFor(state.screen);
  const toastId = state.toast?.id;

  useEffect(() => {
    if (toastId === undefined) return;
    const timer = window.setTimeout(() => dispatch({ type: "dismissToast" }), TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [toastId, dispatch]);

  const closeSettings = useCallback(() => dispatch({ type: "closeSettings" }), [dispatch]);

  return (
    <div className="app">
      {/* While the settings sheet is open, only the sheet can be reached. */}
      <nav className="nav" aria-label={t("nav.main")} inert={state.settingsOpen}>
        <div className="brand">Interlo</div>
        {NAV_ITEMS.map((item) => (
          <Button
            key={item.id}
            variant="ghost"
            aria-current={activeNav === item.id ? "page" : undefined}
            onClick={() => dispatch({ type: "navigate", screen: item.id })}
          >
            {t(item.labelKey)}
          </Button>
        ))}
        <Button
          variant="ghost"
          className="nav__settings"
          onClick={() => dispatch({ type: "openSettings" })}
        >
          {t("nav.settings")}
        </Button>
      </nav>
      <main className="main" inert={state.settingsOpen}>
        {client.kind === "mock" ? <Banner kind="info">{t("app.mockBanner")}</Banner> : null}
        <ScreenOutlet screen={state.screen} meetingId={state.meetingId} />
      </main>
      <SettingsSheet open={state.settingsOpen} onClose={closeSettings} />
      {state.toast ? (
        <Toast
          message={state.toast.message}
          kind={state.toast.kind}
          onDismiss={() => dispatch({ type: "dismissToast" })}
        />
      ) : null}
    </div>
  );
}
