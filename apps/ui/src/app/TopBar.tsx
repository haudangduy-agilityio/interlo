import { useSession } from "../ipc/SessionProvider";
import { useStart } from "../features/start";
import { useAppState } from "./AppState";
import { TopBarView } from "./TopBarView";

/** The top bar with the shell's real state. `stop` comes from the shell, which shows its failure. */
export function TopBar({ onStop }: { onStop: () => void }) {
  const { dispatch } = useAppState();
  const { session } = useSession();
  const { requestStart } = useStart();
  return (
    <TopBarView
      running={session.status === "running" || session.status === "stopping"}
      stopping={session.status === "stopping"}
      starting={session.status === "starting"}
      onStart={(mode) => void requestStart(mode)}
      onReviewSetup={() => dispatch({ type: "navigate", screen: "setup" })}
      onStop={onStop}
      onGoLive={() => dispatch({ type: "navigate", screen: "live" })}
    />
  );
}
