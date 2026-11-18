import { useSession } from "../../ipc/SessionProvider";
import { SubtitleBarView } from "./SubtitleBarView";

export function SubtitleBarScreen() {
  const { session } = useSession();
  return <SubtitleBarView session={session} />;
}
