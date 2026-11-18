import { useT } from "../i18n";
import { Button } from "./Button";
import type { BannerKind } from "./Banner";

export interface ToastProps {
  message: string;
  kind?: BannerKind;
  onDismiss: () => void;
}

/** A short message that goes away by itself. Never the only place a lasting state is shown. */
export function Toast({ message, kind = "info", onDismiss }: ToastProps) {
  const t = useT();
  return (
    <div className={`toast toast--${kind}`} role="status">
      <span>{message}</span>
      <Button variant="ghost" aria-label={t("common.dismissNotification")} onClick={onDismiss}>
        ×
      </Button>
    </div>
  );
}
