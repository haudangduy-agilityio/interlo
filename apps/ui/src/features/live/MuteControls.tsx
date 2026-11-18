import { isPaused, type SessionState } from "../../ipc/meetingSession";
import type { Direction } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import { Button } from "../../shared/ui";

export interface MuteControlsProps {
  session: SessionState;
  onToggleMute: (direction: Direction) => void;
  onTogglePause: () => void;
}

/** Mute each direction, or pause both. Enabled only while a meeting runs. */
export function MuteControls({ session, onToggleMute, onTogglePause }: MuteControlsProps) {
  const t = useT();
  const running = session.status === "running";
  const paused = isPaused(session);
  return (
    <div className="controls" role="group" aria-label={t("mute.group")}>
      <Button
        aria-pressed={session.mutedOut}
        disabled={!running}
        onClick={() => onToggleMute("out")}
      >
        {session.mutedOut ? t("mute.unmuteVoice") : t("mute.muteVoice")}
      </Button>
      <Button aria-pressed={session.mutedIn} disabled={!running} onClick={() => onToggleMute("in")}>
        {session.mutedIn ? t("mute.unmuteAudio") : t("mute.muteAudio")}
      </Button>
      <Button disabled={!running} onClick={onTogglePause}>
        {paused ? t("mute.resume") : t("mute.pause")}
      </Button>
    </div>
  );
}
