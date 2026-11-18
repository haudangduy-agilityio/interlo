import type { MeetingMode } from "../../ipc/types";
import type { MessageKey } from "../../shared/i18n";

/** Screens show `t(labelKey)`. */
export const MODE_OPTIONS: readonly { value: MeetingMode; labelKey: MessageKey }[] = [
  { value: "both", labelKey: "start.mode.both" },
  { value: "listen", labelKey: "start.mode.listen" },
  { value: "speak", labelKey: "start.mode.speak" },
  { value: "off", labelKey: "start.mode.off" },
];
