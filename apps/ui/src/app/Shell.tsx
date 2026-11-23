import { useCallback, useEffect } from "react";
import { HomeScreen } from "../features/home";
import { LiveScreen } from "../features/live";
import { MeetingDetailScreen } from "../features/meeting-detail";
import { MeetingsScreen } from "../features/meetings";
import { SetupScreen } from "../features/setup";
import { WorkspaceScreen } from "../features/workspace";
import { useClient } from "../ipc/ClientProvider";
import { StartProvider } from "../features/start";
import { useT } from "../shared/i18n";
import { ActionErrorBanner, Banner, Toast } from "../shared/ui";
import { useAppState } from "./AppState";
import type { ScreenId } from "./screens";
import { SettingsSheet } from "./SettingsSheet";
import { SetupStatusProvider } from "./SetupStatusProvider";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { useStopMeeting } from "./useStopMeeting";

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

/** The shell of the demo: sidebar, top bar and the open screen. */
function ShellFrame() {
  const { state, dispatch } = useAppState();
  const client = useClient();
  const stopMeeting = useStopMeeting();
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
      <Sidebar />
      <div className="main" inert={state.settingsOpen}>
        <TopBar onStop={() => void stopMeeting.stop()} />
        <main className="view">
          {client.kind === "mock" ? <MockBanner /> : null}
          {stopMeeting.failure.banner ? (
            <ActionErrorBanner {...stopMeeting.failure.banner} />
          ) : null}
          <ScreenOutlet screen={state.screen} meetingId={state.meetingId} />
        </main>
      </div>
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

function MockBanner() {
  const t = useT();
  return <Banner kind="info">{t("app.mockBanner")}</Banner>;
}

export function Shell() {
  return (
    <SetupStatusProvider>
      <StartProvider>
        <ShellFrame />
      </StartProvider>
    </SetupStatusProvider>
  );
}
