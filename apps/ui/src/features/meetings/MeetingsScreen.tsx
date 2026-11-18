import { useState } from "react";
import { useAppState } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { useAsync } from "../../shared/useAsync";
import { MeetingsView } from "./MeetingsView";

export function MeetingsScreen() {
  const client = useClient();
  const { dispatch } = useAppState();
  const { state, reload } = useAsync(() => client.listMeetings(), [client]);
  const [query, setQuery] = useState("");
  return (
    <MeetingsView
      meetings={state}
      query={query}
      onQueryChange={setQuery}
      onClearSearch={() => setQuery("")}
      onOpen={(meetingId) => dispatch({ type: "openMeeting", meetingId })}
      onRetry={reload}
    />
  );
}
