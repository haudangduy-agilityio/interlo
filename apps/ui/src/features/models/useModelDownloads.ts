import { useToast } from "../../app/AppState";
import type { Client } from "../../ipc/client";
import { toUiError } from "../../ipc/errors";
import { useSession } from "../../ipc/SessionProvider";
import type { ModelsProgressEvent } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import type { ActionErrorBannerProps } from "../../shared/ui";
import { useActionFailure } from "../../shared/useActionFailure";
import { useModelLicence, type ModelLicenceApi } from "./useModelLicence";

export interface ModelDownloadsApi {
  licence: ModelLicenceApi;
  /** The latest progress event of each model. */
  progress: Readonly<Record<string, ModelsProgressEvent>>;
  cancel(id: string): void;
  /** The last failed action, or a download that failed in the shell. Shown on the page. */
  error: ActionErrorBannerProps | undefined;
  /** Forget the failure of an action the owner started. */
  clearError(): void;
}

/**
 * Model downloads for Setup and Settings: the command calls, the progress events and the
 * failures. Both screens call it, so they behave the same. `reload` loads the screen's list again;
 * `labelOf` gives the name the screen shows for a model id.
 */
export function useModelDownloads(
  client: Client,
  reload: () => void,
  labelOf: (id: string) => string,
): ModelDownloadsApi {
  const toast = useToast();
  const t = useT();
  const { models, clearModelProgress } = useSession();
  const { banner, fail, clear } = useActionFailure();

  const download = (id: string) => {
    clear();
    clearModelProgress(id);
    // The owner ticked the licence for this model before this call (D-47).
    client.downloadModel({ id, licenceAccepted: true }).then(reload, (e: unknown) => {
      toast(toUiError(e).message, "error");
      fail(e, () => download(id));
    });
  };

  const cancel = (id: string) => {
    clear();
    client.cancelModelDownload({ id }).then(
      () => {
        clearModelProgress(id);
        reload();
      },
      (e: unknown) => {
        toast(toUiError(e).message, "error");
        fail(e);
      },
    );
  };

  const licence = useModelLicence(client, download);

  // A download that failed inside the shell stays on the page until Retry or Dismiss.
  const failed = Object.values(models.byId).find((e) => e.state === "error");
  const eventError: ActionErrorBannerProps | undefined = failed
    ? {
        message: t("models.downloadFailed", { name: labelOf(failed.id) }),
        onRetry: () => download(failed.id),
        onDismiss: () => clearModelProgress(failed.id),
      }
    : undefined;

  return {
    licence,
    progress: models.byId,
    cancel,
    error: banner ?? eventError,
    clearError: clear,
  };
}
