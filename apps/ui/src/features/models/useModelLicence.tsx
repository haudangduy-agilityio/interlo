import { useState, type ReactNode } from "react";
import type { Client } from "../../ipc/client";
import { SETUP_ITEM_LABEL_KEY } from "../../ipc/labels";
import { SETUP_ITEM_IDS, type SetupItemId } from "../../ipc/types";
import { useT, type MessageKey } from "../../shared/i18n";
import { ModelLicenceDialog, type LicenceModel } from "./ModelLicenceDialog";

export interface ModelLicenceApi {
  /** The owner pressed Download: read the model's licence and open the dialog. */
  request(id: string): void;
  /** The open dialog, or null. Render it once, next to the list. */
  dialog: ReactNode;
}

function fallbackModel(id: string, label: (key: MessageKey) => string): LicenceModel {
  const known = (SETUP_ITEM_IDS as readonly string[]).includes(id);
  return {
    id,
    name: known ? label(SETUP_ITEM_LABEL_KEY[id as SetupItemId]) : id,
    licence: "",
    nonCommercial: false,
  };
}

/**
 * Every model download goes through the licence dialog, in Settings and in Setup. The model
 * list is read at the click, so the dialog shows the shell's current licence. When the list
 * cannot be read the dialog still opens, says the licence is not listed (an empty licence), and still needs the tick.
 */
export function useModelLicence(client: Client, download: (id: string) => void): ModelLicenceApi {
  const t = useT();
  const [pending, setPending] = useState<LicenceModel | null>(null);
  const request = (id: string) => {
    client.listModels().then(
      (all) => setPending(all.find((m) => m.id === id) ?? fallbackModel(id, t)),
      () => setPending(fallbackModel(id, t)),
    );
  };
  const dialog = pending ? (
    <ModelLicenceDialog
      model={pending}
      onCancel={() => setPending(null)}
      onConfirm={() => {
        setPending(null);
        download(pending.id);
      }}
    />
  ) : null;
  return { request, dialog };
}
