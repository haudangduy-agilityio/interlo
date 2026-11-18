import { useCallback, useState } from "react";
import type { MeetingMode, StartMeetingArgs } from "../../ipc/types";
import type { MessageKey } from "../../shared/i18n";

/** Generic names. The ids are what `start_meeting` sends as `meetingAppId`. */
export const MEETING_APPS: readonly { id: string; labelKey: MessageKey }[] = [
  { id: "browser", labelKey: "start.app.browser" },
  { id: "desktop", labelKey: "start.app.desktop" },
];

/** What the owner answers in the "Before you start" panel. */
export interface StartForm {
  /** "" until the owner chooses one, which fails the "meetingApp" check. */
  meetingAppId: string;
  headphonesConfirmed: boolean;
  meetingMicConfirmed: boolean;
}

const EMPTY_FORM: StartForm = {
  meetingAppId: "",
  headphonesConfirmed: false,
  meetingMicConfirmed: false,
};

export function toStartArgs(mode: MeetingMode, form: StartForm): StartMeetingArgs {
  return {
    mode,
    meetingAppId: form.meetingAppId,
    checks: {
      headphonesConfirmed: form.headphonesConfirmed,
      meetingMicConfirmed: form.meetingMicConfirmed,
    },
  };
}

export interface StartFormApi {
  open: boolean;
  form: StartForm;
  /** Open the panel with empty answers: the owner confirms again at every Start. */
  begin(): void;
  close(): void;
  change(patch: Partial<StartForm>): void;
}

/** The panel's open state and answers. They live in the screen, so a refused start keeps them. */
export function useStartForm(): StartFormApi {
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState<StartForm>(EMPTY_FORM);
  const begin = useCallback(() => {
    setForm(EMPTY_FORM);
    setOpen(true);
  }, []);
  const close = useCallback(() => setOpen(false), []);
  const change = useCallback(
    (patch: Partial<StartForm>) => setForm((f) => ({ ...f, ...patch })),
    [],
  );
  return { open, form, begin, close, change };
}
