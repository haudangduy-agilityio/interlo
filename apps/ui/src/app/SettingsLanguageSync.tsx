import { useEffect } from "react";
import { useClient } from "../ipc/ClientProvider";
import { useLanguage } from "../shared/i18n";

/**
 * Reads the saved interface language once when the app starts and gives it to the language
 * provider. If the settings cannot be read, the app stays in English.
 */
export function SettingsLanguageSync() {
  const client = useClient();
  const { setLanguage } = useLanguage();
  useEffect(() => {
    let cancelled = false;
    client.getSettings().then(
      (settings) => {
        if (!cancelled) setLanguage(settings.uiLanguage);
      },
      () => {
        // Keep English: the General tab shows the failure when the owner opens it.
      },
    );
    return () => {
      cancelled = true;
    };
  }, [client, setLanguage]);
  return null;
}
