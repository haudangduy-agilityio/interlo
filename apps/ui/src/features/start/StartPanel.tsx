import { START_CHECK_FIX_KEY } from "../../ipc/labels";
import type { StartFailure } from "../../ipc/startOutcome";
import type { MeetingMode } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import { Banner, Button, Section } from "../../shared/ui";
import { MODE_OPTIONS } from "./modeOptions";
import { MEETING_APPS, type StartForm } from "./startForm";

export interface StartPanelProps {
  form: StartForm;
  onChange: (patch: Partial<StartForm>) => void;
  /**
   * The last refused start. Failed per-meeting checks show as fix steps. Live shows its other
   * refusals itself, so it passes only the failed checks; Home passes every refusal.
   */
  failure: StartFailure | null;
  /** A start request is waiting for the shell. */
  busy: boolean;
  /** Pass both to show the mode choice inside the panel (Home). Live has its own switch. */
  mode?: MeetingMode;
  onModeChange?: (mode: MeetingMode) => void;
  onConfirm: () => void;
  onCancel: () => void;
}

function Refusal({ failure }: { failure: StartFailure }) {
  const t = useT();
  if (failure.kind !== "checks-failed") return <Banner kind="error">{failure.message}</Banner>;
  return (
    <Banner
      kind="error"
      details={
        <ul className="banner__steps" aria-label={t("start.fixSteps")}>
          {failure.details.map((id) => (
            <li key={id}>{t(START_CHECK_FIX_KEY[id])}</li>
          ))}
        </ul>
      }
    >
      {failure.message}
    </Banner>
  );
}

/** The fields and buttons of the per-meeting checks (D-44). The panel and the dialog wrap them. */
export function StartPanelBody({
  form,
  onChange,
  failure,
  busy,
  mode,
  onModeChange,
  onConfirm,
  onCancel,
}: StartPanelProps) {
  const t = useT();
  return (
    <>
      {failure ? <Refusal failure={failure} /> : null}
      {mode !== undefined && onModeChange ? (
        <label className="field">
          <span>{t("live.mode")}</span>
          <select value={mode} onChange={(e) => onModeChange(e.target.value as MeetingMode)}>
            {MODE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {t(o.labelKey)}
              </option>
            ))}
          </select>
        </label>
      ) : null}
      <label className="field">
        <span>{t("start.meetingApp")}</span>
        <select
          value={form.meetingAppId}
          onChange={(e) => onChange({ meetingAppId: e.target.value })}
        >
          <option value="">{t("start.chooseApp")}</option>
          {MEETING_APPS.map((app) => (
            <option key={app.id} value={app.id}>
              {t(app.labelKey)}
            </option>
          ))}
        </select>
      </label>
      <label className="field field--inline">
        <input
          type="checkbox"
          checked={form.headphonesConfirmed}
          onChange={(e) => onChange({ headphonesConfirmed: e.target.checked })}
        />
        <span>{t("start.headphones")}</span>
      </label>
      <label className="field field--inline">
        <input
          type="checkbox"
          checked={form.meetingMicConfirmed}
          onChange={(e) => onChange({ meetingMicConfirmed: e.target.checked })}
        />
        <span>{t("start.meetingMic")}</span>
      </label>
      <div className="row">
        <Button variant="primary" disabled={busy} onClick={onConfirm}>
          {t("start.now")}
        </Button>
        <Button onClick={onCancel}>{t("common.cancel")}</Button>
      </div>
    </>
  );
}

/** The per-meeting checks the owner confirms at every Start (D-44). Without hooks. */
export function StartPanel(props: StartPanelProps) {
  const t = useT();
  return (
    <Section title={t("start.title")}>
      <StartPanelBody {...props} />
    </Section>
  );
}
