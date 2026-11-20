import { useAppState } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { useClock } from "../../shared/clock";
import { useAsync } from "../../shared/useAsync";
import { useStart } from "../start";
import { HomeView } from "./HomeView";

export function HomeScreen() {
  const client = useClient();
  const clock = useClock();
  const { dispatch } = useAppState();
  const { requestStart } = useStart();
  const meetings = useAsync(() => client.listMeetings(), [client]);
  const notes = useAsync(() => client.listNotes(), [client]);

  return (
    <HomeView
      nowMs={clock.now()}
      meetings={meetings.state}
      notes={notes.state}
      onRetryMeetings={meetings.reload}
      onRetryNotes={notes.reload}
      onStartMeeting={() => void requestStart()}
      onOpenMeeting={(meetingId) => dispatch({ type: "openMeeting", meetingId })}
      onSeeAllMeetings={() => dispatch({ type: "navigate", screen: "meetings" })}
      onOpenWorkspace={() => dispatch({ type: "navigate", screen: "workspace" })}
    />
  );
}
