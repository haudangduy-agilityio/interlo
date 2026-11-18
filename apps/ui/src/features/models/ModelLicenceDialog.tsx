import { useState } from "react";
import { formatBytes } from "../../shared/format";
import { useT } from "../../shared/i18n";
import { Button, Dialog } from "../../shared/ui";

/** What the dialog shows about one model. An empty `licence` means none is listed. */
export interface LicenceModel {
  id: string;
  name: string;
  sizeBytes?: number;
  licence: string;
  nonCommercial: boolean;
}

export interface ModelLicenceDialogProps {
  model: LicenceModel;
  onConfirm: () => void;
  onCancel: () => void;
}

/** D-47: the owner reads the licence and accepts it before a model is downloaded. */
export function ModelLicenceDialog({ model, onConfirm, onCancel }: ModelLicenceDialogProps) {
  // Local on purpose: every open starts unticked, so acceptance is never carried over.
  const [accepted, setAccepted] = useState(false);
  const t = useT();
  return (
    <Dialog title={t("licence.title", { name: model.name })} onClose={onCancel}>
      <div className="stack">
        {model.sizeBytes !== undefined ? (
          <p className="muted">{t("licence.size", { size: formatBytes(model.sizeBytes) })}</p>
        ) : null}
        <p>
          {t("licence.line", {
            licence: model.licence === "" ? t("licence.notListed") : model.licence,
          })}
        </p>
        {model.nonCommercial ? (
          <>
            <span className="chip">{t("licence.chip")}</span>
            <p className="muted">{t("licence.chipBody")}</p>
          </>
        ) : null}
        <label className="field field--inline">
          <input
            type="checkbox"
            checked={accepted}
            onChange={(e) => setAccepted(e.target.checked)}
          />
          <span>{t("licence.accept")}</span>
        </label>
        <div className="row">
          <Button variant="primary" disabled={!accepted} onClick={onConfirm}>
            {t("common.download")}
          </Button>
          <Button onClick={onCancel}>{t("common.cancel")}</Button>
        </div>
      </div>
    </Dialog>
  );
}
