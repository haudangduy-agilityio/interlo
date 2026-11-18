import { isActiveDownload } from "../../ipc/modelsProgress";
import type { ModelInfo, ModelsProgressEvent, ModelState } from "../../ipc/types";
import { formatBytes } from "../../shared/format";
import { useT, type MessageKey } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import { DownloadProgress } from "../models";
import {
  ActionErrorBanner,
  AsyncContent,
  Button,
  EmptyState,
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

function ModelRow({ model, progress, onDownload, onCancel }: RowHandlers & { model: ModelInfo }) {
  const t = useT();
  const event = progress[model.id];
  // The list can say "downloading" before the first event, so it counts too.
  const active = model.state !== "ok" && (isActiveDownload(event) || model.state === "downloading");
  const canDownload = !active && (model.state === "missing" || model.state === "error");
  return (
    <li className="list__row">
      <span>{model.name}</span>
      <span className="muted">{formatBytes(model.sizeBytes)}</span>
      <span className={`badge badge--${model.state}`}>{t(STATE_LABEL[model.state])}</span>
      {canDownload ? (
        <Button
          aria-label={t("setup.downloadNamed", { name: model.name })}
          onClick={() => onDownload(model.id)}
        >
          {t("common.download")}
        </Button>
      ) : null}
      {active ? (
        <DownloadProgress
          label={model.name}
          event={isActiveDownload(event) ? event : undefined}
          onCancel={() => onCancel(model.id)}
        />
      ) : null}
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
}: RowHandlers & { models: Async<ModelInfo[]>; onRetry: () => void }) {
  const t = useT();
  return (
    <AsyncContent
      state={models}
      onRetry={onRetry}
      loadingLabel={t("models.loading")}
      isEmpty={(all) => all.length === 0}
      empty={<EmptyState title={t("models.emptyTitle")} description={t("models.emptyBody")} />}
    >
      {(all) => (
        <ul className="list" aria-label={t("models.listLabel")}>
          {all.map((m) => (
            <ModelRow
              key={m.id}
              model={m}
              progress={progress}
              onDownload={onDownload}
              onCancel={onCancel}
            />
          ))}
        </ul>
      )}
    </AsyncContent>
  );
}
