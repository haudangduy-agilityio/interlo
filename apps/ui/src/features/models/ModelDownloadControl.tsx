import { isActiveDownload } from "../../ipc/modelsProgress";
import type { ModelsProgressEvent, ModelState, SetupItemState } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import { Button } from "../../shared/ui";
import { DownloadProgress } from "./DownloadProgress";

export interface ModelDownloadControlProps {
  id: string;
  /** The model name used in the accessible names, for example "English voice model". */
  label: string;
  /** The state the model list or the setup list reports. */
  state: ModelState | SetupItemState;
  /** The latest progress event of this model, if one arrived. */
  event?: ModelsProgressEvent;
  onDownload: (id: string) => void;
  onCancel: (id: string) => void;
}

/**
 * What a model row offers next: a Download button when the model is missing or
 * failed, or a progress bar with Cancel while a download runs. Nothing when the
 * model is ready. The Models tab and the setup list both use it.
 */
export function ModelDownloadControl({
  id,
  label,
  state,
  event,
  onDownload,
  onCancel,
}: ModelDownloadControlProps) {
  const t = useT();
  // The list can say "downloading" or "installing" before the first event, so that counts too.
  const listSaysRunning = state === "downloading" || state === "installing";
  const active = state !== "ok" && (isActiveDownload(event) || listSaysRunning);
  const canDownload = !active && (state === "missing" || state === "error");
  return (
    <>
      {canDownload ? (
        <Button
          aria-label={t("setup.downloadNamed", { name: label })}
          onClick={() => onDownload(id)}
        >
          {t("common.download")}
        </Button>
      ) : null}
      {active ? (
        <DownloadProgress
          label={label}
          event={isActiveDownload(event) ? event : undefined}
          onCancel={() => onCancel(id)}
        />
      ) : null}
    </>
  );
}
