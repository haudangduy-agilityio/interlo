import { GROUP_LABEL_KEY, GROUP_ORDER, SETUP_ITEM_LABEL_KEY, setupFixKey } from "../../ipc/labels";
import type { ModelsProgressEvent, SetupItem, SetupItemState, SetupStatus } from "../../ipc/types";
import { useState } from "react";
import { useT, type MessageKey } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import { ModelDownloadControl } from "../models";
import {
  ActionErrorBanner,
  AsyncContent,
  Banner,
  Button,
  EmptyState,
  Page,
  Section,
  type ActionErrorBannerProps,
} from "../../shared/ui";

export interface SetupViewProps {
  status: Async<SetupStatus>;
  /** The last failed action, such as a refused download. */
  error?: ActionErrorBannerProps;
  /** The latest `models://progress` event of each model. */
  progress?: Readonly<Record<string, ModelsProgressEvent>>;
  onRecheck: () => void;
  onDownload: (id: string) => void;
  onCancel?: (id: string) => void;
  canDownloadModels?: boolean;
}

const NO_PROGRESS: Readonly<Record<string, ModelsProgressEvent>> = {};
const NO_CANCEL = () => {};

const STATE_LABEL: Record<SetupItemState, MessageKey> = {
  ok: "setup.state.ok",
  missing: "setup.state.missing",
  installing: "setup.state.installing",
  error: "setup.state.error",
};

// The app cannot install a virtual audio device itself, so the owner follows these steps.
const VIRTUAL_MIC_STEPS: readonly MessageKey[] = ["setup.step1", "setup.step2", "setup.step3"];

function SetupSteps() {
  const t = useT();
  return (
    <ol className="steps" aria-label={t("setup.stepsLabel")}>
      {VIRTUAL_MIC_STEPS.map((step) => (
        <li key={step}>{t(step)}</li>
      ))}
    </ol>
  );
}

interface ItemRowProps {
  item: SetupItem;
  progress: Readonly<Record<string, ModelsProgressEvent>>;
  onDownload: (id: string) => void;
  onCancel: (id: string) => void;
  canDownloadModels: boolean;
}

function ItemRow({ item, progress, onDownload, onCancel, canDownloadModels }: ItemRowProps) {
  const [stepsOpen, setStepsOpen] = useState(false);
  const t = useT();
  const label = t(SETUP_ITEM_LABEL_KEY[item.id]);
  const notReady = item.state === "missing" || item.state === "error";
  const needsSteps = item.id === "vmic" && notReady;
  const downloadBlocked = item.group === "models" && notReady && !canDownloadModels;
  const fixKey = downloadBlocked ? "models.downloadUnavailable" : setupFixKey(item);
  return (
    <li className="list__row">
      <span>{label}</span>
      <span className="muted">{item.detail}</span>
      <span className={`badge badge--${item.state}`}>{t(STATE_LABEL[item.state])}</span>
      {item.group === "models" ? (
        <ModelDownloadControl
          id={item.id}
          label={label}
          state={item.state}
          event={progress[item.id]}
          onDownload={onDownload}
          onCancel={onCancel}
          unavailable={!canDownloadModels}
        />
      ) : null}
      {needsSteps ? (
        <Button aria-expanded={stepsOpen} onClick={() => setStepsOpen((open) => !open)}>
          {stepsOpen ? t("setup.hideSteps") : t("setup.showSteps")}
        </Button>
      ) : null}
      {item.state !== "ok" ? <p className="setup-fix muted">{t(fixKey)}</p> : null}
      {needsSteps && stepsOpen ? <SetupSteps /> : null}
    </li>
  );
}

export function SetupView({
  status,
  error,
  progress = NO_PROGRESS,
  onRecheck,
  onDownload,
  onCancel = NO_CANCEL,
  canDownloadModels = true,
}: SetupViewProps) {
  const t = useT();
  return (
    <Page
      title={t("nav.setup")}
      actions={<Button onClick={onRecheck}>{t("setup.checkAgain")}</Button>}
    >
      {error ? <ActionErrorBanner {...error} /> : null}
      <AsyncContent
        state={status}
        onRetry={onRecheck}
        loadingLabel={t("setup.checking")}
        isEmpty={(s) => s.items.length === 0}
        empty={<EmptyState title={t("setup.emptyTitle")} description={t("setup.emptyBody")} />}
      >
        {(s) => (
          <>
            {s.canStart ? (
              <Banner kind="info">{t("setup.complete")}</Banner>
            ) : (
              <Banner kind="warning">
                {t("setup.notOnYet", { groups: s.missingGroups.join(", ") })}
              </Banner>
            )}
            {GROUP_ORDER.map((group) => {
              const items = s.items.filter((i) => i.group === group);
              if (items.length === 0) return null;
              return (
                <Section key={group} title={t(GROUP_LABEL_KEY[group])}>
                  <ul className="list" aria-label={t(GROUP_LABEL_KEY[group])}>
                    {items.map((i) => (
                      <ItemRow
                        key={i.id}
                        item={i}
                        progress={progress}
                        onDownload={onDownload}
                        onCancel={onCancel}
                        canDownloadModels={canDownloadModels}
                      />
                    ))}
                  </ul>
                </Section>
              );
            })}
          </>
        )}
      </AsyncContent>
    </Page>
  );
}
