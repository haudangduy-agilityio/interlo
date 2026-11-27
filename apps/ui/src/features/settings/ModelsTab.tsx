import type { ModelInfo, ModelsProgressEvent, ModelState } from "../../ipc/types";
import { formatBytes } from "../../shared/format";
import { useT, type MessageKey } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import { ModelDownloadControl } from "../models";
import {
  ActionErrorBanner,
  AsyncContent,
  EmptyState,
  NotAvailable,
  type ActionErrorBannerProps,
} from "../../shared/ui";

export interface ModelsTabProps {
  models: Async<ModelInfo[]>;
  /** The last failed download. */
  error?: ActionErrorBannerProps;
  /** The latest `models://progress` event of each model. */
  progress?: Readonly<Record<string, ModelsProgressEvent>>;
  onDownload: (id: string) => void;
  onCancel?: (id: string) => void;
  onRetry: () => void;
  canDownloadModels?: boolean;
}

interface RowHandlers {
  progress: Readonly<Record<string, ModelsProgressEvent>>;
  onDownload: (id: string) => void;
  onCancel: (id: string) => void;
}

const STATE_LABEL: Record<ModelState, MessageKey> = {
  ok: "models.state.ok",
  missing: "models.state.missing",
  downloading: "models.state.downloading",
  error: "models.state.error",
};

function ModelRow({
  model,
  progress,
  onDownload,
  onCancel,
  unavailable,
}: RowHandlers & { model: ModelInfo; unavailable: boolean }) {
  const t = useT();
  return (
    <li className="list__row">
      <span>{model.name}</span>
      <span className="muted">{formatBytes(model.sizeBytes)}</span>
      <span className={`badge badge--${model.state}`}>{t(STATE_LABEL[model.state])}</span>
      <ModelDownloadControl
        id={model.id}
        label={model.name}
        state={model.state}
        event={progress[model.id]}
        onDownload={onDownload}
        onCancel={onCancel}
        unavailable={unavailable}
      />
    </li>
  );
}

const NO_PROGRESS: Readonly<Record<string, ModelsProgressEvent>> = {};
const NO_CANCEL = () => {};

export function ModelsTab({
  models,
  error,
  progress = NO_PROGRESS,
  onDownload,
  onCancel = NO_CANCEL,
  onRetry,
  canDownloadModels = true,
}: ModelsTabProps) {
  return (
    <div className="stack">
      {error ? <ActionErrorBanner {...error} /> : null}
      <ModelsList
        models={models}
        progress={progress}
        onDownload={onDownload}
        onCancel={onCancel}
        onRetry={onRetry}
        canDownloadModels={canDownloadModels}
      />
    </div>
  );
}

function ModelsList({
  models,
  progress,
  onDownload,
  onCancel,
  onRetry,
  canDownloadModels,
}: RowHandlers & { models: Async<ModelInfo[]>; onRetry: () => void; canDownloadModels: boolean }) {
  const t = useT();
  if (models.status === "error" && !canDownloadModels && models.error.code === "NOT_IMPLEMENTED") {
    return <NotAvailable>{t("models.downloadUnavailable")}</NotAvailable>;
  }
  return (
    <AsyncContent
      state={models}
      onRetry={onRetry}
      loadingLabel={t("models.loading")}
      isEmpty={(all) => all.length === 0}
      empty={<EmptyState title={t("models.emptyTitle")} description={t("models.emptyBody")} />}
    >
      {(all) => (
        <>
          {canDownloadModels ? null : <p className="muted">{t("models.downloadUnavailable")}</p>}
          <ul className="list" aria-label={t("models.listLabel")}>
            {all.map((m) => (
              <ModelRow
                key={m.id}
                model={m}
                progress={progress}
                onDownload={onDownload}
                onCancel={onCancel}
                unavailable={!canDownloadModels}
              />
            ))}
          </ul>
        </>
      )}
    </AsyncContent>
  );
}
