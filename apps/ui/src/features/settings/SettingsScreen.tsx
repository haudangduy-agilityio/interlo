import { useEffect, useState } from "react";
import { useToast } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { toUiError } from "../../ipc/errors";
import { useSession } from "../../ipc/SessionProvider";
import type { SettingKey, SettingValue } from "../../ipc/types";
import { useLanguage, useT, type MessageKey } from "../../shared/i18n";
import { useActionFailure } from "../../shared/useActionFailure";
import { useAsync } from "../../shared/useAsync";
import { Tabs } from "../../shared/ui";
import { useModelDownloads } from "../models";
import { GeneralTab } from "./GeneralTab";
import { ModelsTab } from "./ModelsTab";
import { HotkeysTab, VocabularyTab, VoicesTab } from "./StaticTabs";

export const SETTINGS_TABS = [
  { id: "general", labelKey: "settings.tab.general" },
  { id: "voices", labelKey: "settings.tab.voices" },
  { id: "vocabulary", labelKey: "settings.tab.vocabulary" },
  { id: "hotkeys", labelKey: "settings.tab.hotkeys" },
  { id: "models", labelKey: "settings.tab.models" },
] as const satisfies readonly { id: string; labelKey: MessageKey }[];
export type SettingsTabId = (typeof SETTINGS_TABS)[number]["id"];

function GeneralPanel() {
  const client = useClient();
  const toast = useToast();
  const { state, reload, setData } = useAsync(() => client.getSettings(), [client]);
  const { banner, fail, clear } = useActionFailure();
  const { setLanguage } = useLanguage();
  // The interface follows the setting: a change shows at once, and a refused change goes back
  // with the value the shell kept (the list is loaded again after a failure).
  const savedLanguage = state.status === "done" ? state.data.uiLanguage : null;
  useEffect(() => {
    if (savedLanguage !== null) setLanguage(savedLanguage);
  }, [savedLanguage, setLanguage]);
  const change = (key: SettingKey, value: SettingValue) => {
    clear();
    setData((s) => ({ ...s, [key]: value }));
    client.setSetting({ key, value }).catch((e: unknown) => {
      toast(toUiError(e).message, "error");
      fail(e);
      // Show the value the shell kept, not the refused one.
      reload();
    });
  };
  return <GeneralTab settings={state} error={banner} onChange={change} onRetry={reload} />;
}

function ModelsPanel() {
  const client = useClient();
  const { models } = useSession();
  // `finished` loads the list again when a download ends, so the row shows its new state.
  const { state, reload } = useAsync(() => client.listModels(), [client, models.finished]);
  const labelOf = (id: string) =>
    (state.status === "done" ? state.data.find((m) => m.id === id)?.name : undefined) ?? id;
  const downloads = useModelDownloads(client, reload, labelOf);
  return (
    <>
      <ModelsTab
        models={state}
        error={downloads.error}
        progress={downloads.progress}
        onDownload={downloads.licence.request}
        onCancel={downloads.cancel}
        onRetry={reload}
      />
      {downloads.licence.dialog}
    </>
  );
}

function TabPanel({ tab }: { tab: SettingsTabId }) {
  switch (tab) {
    case "general":
      return <GeneralPanel />;
    case "voices":
      return <VoicesTab />;
    case "vocabulary":
      return <VocabularyTab />;
    case "hotkeys":
      return <HotkeysTab />;
    case "models":
      return <ModelsPanel />;
  }
}

/** Settings content with its tabs. The shell shows it inside a sheet. */
export function SettingsScreen({ initialTab = "general" }: { initialTab?: SettingsTabId }) {
  const t = useT();
  const [tab, setTab] = useState<SettingsTabId>(initialTab);
  return (
    <div className="settings">
      <Tabs
        label={t("settings.sections")}
        tabs={SETTINGS_TABS.map((s) => ({ id: s.id, label: t(s.labelKey) }))}
        active={tab}
        onChange={setTab}
      />
      <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
        <TabPanel tab={tab} />
      </div>
    </div>
  );
}
