import { useEffect } from "react";
import { useSetupStatus } from "../../app/SetupStatusProvider";
import { useClient } from "../../ipc/ClientProvider";
import { SETUP_ITEM_LABEL_KEY } from "../../ipc/labels";
import { useSession } from "../../ipc/SessionProvider";
import { SETUP_ITEM_IDS, type SetupItemId } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import { useAsync } from "../../shared/useAsync";
import { useModelDownloads } from "../models";
import { SetupView } from "./SetupView";

export function SetupScreen() {
  const client = useClient();
  const { models } = useSession();
  const t = useT();
  const labelOf = (id: string): string =>
    (SETUP_ITEM_IDS as readonly string[]).includes(id)
      ? t(SETUP_ITEM_LABEL_KEY[id as SetupItemId])
      : id;
  // `finished` loads the list again when a download ends, so the row shows its new state.
  const { state, reload } = useAsync(() => client.checkSetup(), [client, models.finished]);
  const downloads = useModelDownloads(client, reload, labelOf);
  // The sidebar badge and the Start gate read the shell's copy of the setup check. Keep it current.
  const { publish } = useSetupStatus();
  useEffect(() => {
    if (state.status === "done") publish(state.data);
  }, [state, publish]);

  return (
    <>
      <SetupView
        status={state}
        error={downloads.error}
        progress={downloads.progress}
        onRecheck={() => {
          downloads.clearError();
          reload();
        }}
        onDownload={downloads.licence.request}
        onCancel={downloads.cancel}
        canDownloadModels={client.canDownloadModels}
      />
      {downloads.licence.dialog}
    </>
  );
}
