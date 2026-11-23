import { useSession } from "../ipc/SessionProvider";
import { useAppState } from "./AppState";
import { navIdFor } from "./screens";
import { useSetupStatus } from "./SetupStatusProvider";
import { SidebarView, type LocalSetupState } from "./SidebarView";

/** The sidebar with the shell's real state: the open screen, the setup check, the meeting. */
export function Sidebar() {
  const { state, dispatch } = useAppState();
  const { state: setup } = useSetupStatus();
  const { session } = useSession();

  let setupCount: number | null = null;
  let setupState: LocalSetupState = setup.status === "loading" ? "checking" : "unknown";
  if (setup.status === "done") {
    setupCount = setup.data.items.filter((i) => i.state !== "ok").length;
    setupState = setup.data.canStart ? "ready" : "missing";
  }

  return (
    <SidebarView
      active={navIdFor(state.screen)}
      onNavigate={(screen) => dispatch({ type: "navigate", screen })}
      onOpenSettings={() => dispatch({ type: "openSettings" })}
      setupCount={setupCount}
      setupState={setupState}
      meetingRunning={session.status === "running" || session.status === "stopping"}
      inert={state.settingsOpen}
    />
  );
}
