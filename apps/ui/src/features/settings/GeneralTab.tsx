import type { AppSettings, SettingKey, SettingValue } from "../../ipc/types";
import { useT, type MessageKey } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import { ActionErrorBanner, AsyncContent, type ActionErrorBannerProps } from "../../shared/ui";

export interface GeneralTabProps {
  settings: Async<AppSettings>;
  /** The last refused change. */
  error?: ActionErrorBannerProps;
  onChange: (key: SettingKey, value: SettingValue) => void;
  onRetry: () => void;
}

/** `days` is null for "never". */
const DELETE_OPTIONS: readonly { value: string; days: number | null }[] = [
  { value: "never", days: null },
  { value: "7", days: 7 },
  { value: "30", days: 30 },
  { value: "90", days: 90 },
];

const LANGUAGE_LABEL: Record<AppSettings["uiLanguage"], MessageKey> = {
  en: "settings.language.en",
  vi: "settings.language.vi",
};

export function GeneralTab({ settings, error, onChange, onRetry }: GeneralTabProps) {
  const t = useT();
  return (
    <div className="stack">
      {error ? <ActionErrorBanner {...error} /> : null}
      <AsyncContent state={settings} onRetry={onRetry} loadingLabel={t("settings.loading")}>
        {(s) => (
          <div className="stack">
            <label className="field">
              <span>{t("settings.language")}</span>
              <select
                value={s.uiLanguage}
                onChange={(e) =>
                  onChange("uiLanguage", e.target.value as AppSettings["uiLanguage"])
                }
              >
                <option value="en">{t(LANGUAGE_LABEL.en)}</option>
                <option value="vi">{t(LANGUAGE_LABEL.vi)}</option>
              </select>
            </label>
            <label className="field field--inline">
              <input
                type="checkbox"
                checked={s.ducking}
                onChange={(e) => onChange("ducking", e.target.checked)}
              />
              <span>{t("settings.ducking")}</span>
            </label>
            <label className="field">
              <span>{t("settings.autoDelete")}</span>
              <select
                value={s.autoDeleteDays === null ? "never" : String(s.autoDeleteDays)}
                onChange={(e) =>
                  onChange(
                    "autoDeleteDays",
                    e.target.value === "never" ? null : Number(e.target.value),
                  )
                }
              >
                {DELETE_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.days === null
                      ? t("settings.autoDelete.never")
                      : t("settings.autoDelete.days", { days: o.days })}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}
      </AsyncContent>
    </div>
  );
}
