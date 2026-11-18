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
      />
      {downloads.licence.dialog}
    </>
  );
}
