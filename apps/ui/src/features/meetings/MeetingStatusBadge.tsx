import type { MeetingStatus } from "../../ipc/types";
import { useT, type MessageKey } from "../../shared/i18n";

const LABEL: Record<MeetingStatus, MessageKey> = {
  ready: "meetingStatus.ready",
  generating: "meetingStatus.generating",
  interrupted: "meetingStatus.interrupted",
  failed: "meetingStatus.failed",
};

export function MeetingStatusBadge({ status }: { status: MeetingStatus }) {
  const t = useT();
  return <span className={`badge badge--${status}`}>{t(LABEL[status])}</span>;
}
