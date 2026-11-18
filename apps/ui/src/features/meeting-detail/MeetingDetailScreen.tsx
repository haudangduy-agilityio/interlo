import { useAppState } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { mapAsync, useAsync } from "../../shared/useAsync";
import { MeetingDetailView } from "./MeetingDetailView";

export function MeetingDetailScreen({ meetingId }: { meetingId: string | null }) {
  const client = useClient();
  const { dispatch } = useAppState();
  // The contract has no get_meeting command, so the detail is read from the list.
  const { state, reload } = useAsync(() => client.listMeetings(), [client, meetingId]);
  const meeting = mapAsync(state, (all) => all.find((m) => m.id === meetingId) ?? null);
  return (
    <MeetingDetailView
      meeting={meeting}
      onBack={() => dispatch({ type: "navigate", screen: "meetings" })}
      onRetry={reload}
    />
  );
}
