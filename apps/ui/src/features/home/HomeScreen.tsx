import { useEffect, useState } from "react";
import { useAppState } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { useSession } from "../../ipc/SessionProvider";
import type { MeetingMode } from "../../ipc/types";
import { useAsync } from "../../shared/useAsync";
import { StartPanel, toStartArgs, useStartForm } from "../start";
import { HomeView } from "./HomeView";

export function HomeScreen() {
  const client = useClient();
  const { dispatch } = useAppState();
  const { session, start, clearStartFailure } = useSession();
  const setup = useAsync(() => client.checkSetup(), [client]);
  const meetings = useAsync(() => client.listMeetings(), [client]);
  const startForm = useStartForm();
  const [mode, setMode] = useState<MeetingMode>("both");

  // A failed start belongs to this visit. Leaving the screen forgets it.
  useEffect(() => clearStartFailure, [clearStartFailure]);

  const confirmStart = async () => {
    const outcome = await start(toStartArgs(mode, startForm.form));
    if (outcome.kind === "started") dispatch({ type: "navigate", screen: "live" });
  };

  return (
    <HomeView
      setup={setup.state}
      meetings={meetings.state}
      onRetrySetup={setup.reload}
      onRetryMeetings={meetings.reload}
      onStartMeeting={startForm.begin}
      onGoToLive={() => dispatch({ type: "navigate", screen: "live" })}
      startPanel={
        startForm.open ? (
          <StartPanel
            form={startForm.form}
            onChange={startForm.change}
            failure={session.startFailure}
            busy={session.status === "starting"}
            mode={mode}
            onModeChange={setMode}
            onConfirm={() => void confirmStart()}
            onCancel={() => {
              clearStartFailure();
              startForm.close();
            }}
          />
        ) : null
      }
      onOpenSetup={() => dispatch({ type: "navigate", screen: "setup" })}
      onOpenMeeting={(meetingId) => dispatch({ type: "openMeeting", meetingId })}
      onSeeAllMeetings={() => dispatch({ type: "navigate", screen: "meetings" })}
    />
  );
}
