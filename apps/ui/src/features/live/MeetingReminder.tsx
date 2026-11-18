import { useState } from "react";
import { useNow } from "../../shared/clock";
import { useT } from "../../shared/i18n";
import { Banner, Button } from "../../shared/ui";
import { reminderRound } from "./reminder";

/**
 * The 10-minute reminder. It is its own component so the clock tick re-renders only this banner,
 * not the caption list. Dismissing it hides the current round; the next round shows it again.
 */
export function MeetingReminder({ startedAtMs }: { startedAtMs: number }) {
  const t = useT();
  const now = useNow();
  const [dismissedRound, setDismissedRound] = useState(0);
  const round = reminderRound(startedAtMs, now);
  if (round === 0 || round <= dismissedRound) return null;
  return (
    <Banner
      kind="info"
      actions={
        <Button
          variant="ghost"
          aria-label={t("live.dismissReminder")}
          onClick={() => setDismissedRound(round)}
        >
          ×
        </Button>
      }
    >
      {t("live.reminder")}
    </Banner>
  );
}
