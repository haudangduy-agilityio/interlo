import { useToast } from "../../app/AppState";
import { toUiError } from "../../ipc/errors";
import { isPaused } from "../../ipc/meetingSession";
import { useSession } from "../../ipc/SessionProvider";
import { useActionFailure } from "../../shared/useActionFailure";
import { CompactPanelView } from "./CompactPanelView";

export function CompactPanelScreen() {
  const { session, stop, setMute, setPaused } = useSession();
  const toast = useToast();
  const { banner, fail, clear } = useActionFailure();
  const run = (action: () => Promise<unknown>) => {
    clear();
    action().catch((e: unknown) => {
      toast(toUiError(e).message, "error");
      fail(e);
    });
  };
  return (
    <CompactPanelView
      session={session}
      error={banner}
      onToggleMute={(direction) => {
        const muted = direction === "out" ? session.mutedOut : session.mutedIn;
        run(() => setMute(direction, !muted));
      }}
      onTogglePause={() => run(() => setPaused(!isPaused(session)))}
      onStop={() => run(stop)}
    />
  );
}
