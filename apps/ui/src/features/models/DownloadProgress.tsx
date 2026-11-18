import { useLayoutEffect, useRef } from "react";
import { downloadPercent } from "../../ipc/modelsProgress";
import type { ModelsProgressEvent } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import { Button } from "../../shared/ui";

export interface DownloadProgressProps {
  /** The model name used in the accessible names, for example "English voice model". */
  label: string;
  /** The latest progress event. Absent when only the model list says it is downloading. */
  event?: ModelsProgressEvent;
  onCancel: () => void;
}

/** A progress bar and a Cancel button for one running model download. */
export function DownloadProgress({ label, event, onCancel }: DownloadProgressProps) {
  const t = useT();
  const percent = event ? downloadPercent(event) : null;
  const verifying = event?.state === "verifying";
  // The width is set through the element's style object, not a `style` attribute:
  // the CSP has no 'unsafe-inline' for styles, and this route is allowed.
  const fill = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (fill.current) fill.current.style.width = percent === null ? "" : `${percent}%`;
  }, [percent]);
  return (
    <div className="download">
      <div
        role="progressbar"
        aria-label={t("models.progressLabel", { name: label })}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
        className={percent === null ? "progress progress--unknown" : "progress"}
      >
        <div ref={fill} className="progress__fill" />
      </div>
      <span className="muted">
        {verifying
          ? t("models.verifying")
          : percent === null
            ? t("models.starting")
            : `${percent}%`}
      </span>
      <Button aria-label={t("models.cancelDownload", { name: label })} onClick={onCancel}>
        {t("common.cancel")}
      </Button>
    </div>
  );
}
