import { useT } from "../../shared/i18n";
import { NotAvailable } from "../../shared/ui";

// Voices, vocabulary and hotkeys have no command in the IPC contract yet.

export function VoicesTab() {
  const t = useT();
  return <NotAvailable>{t("settings.voices")}</NotAvailable>;
}

export function VocabularyTab() {
  const t = useT();
  return <NotAvailable>{t("settings.vocabulary")}</NotAvailable>;
}

export function HotkeysTab() {
  const t = useT();
  return <NotAvailable>{t("settings.hotkeys")}</NotAvailable>;
}
