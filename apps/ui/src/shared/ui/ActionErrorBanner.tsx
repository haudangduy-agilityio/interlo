import { useT } from "../i18n";
import { Banner } from "./Banner";
import { Button } from "./Button";

export interface ActionErrorBannerProps {
  message: string;
  /** Shown only when trying the same action again can work. */
  onRetry?: () => void;
  onDismiss: () => void;
}

/** A command failed. The message stays on the page until the user retries or dismisses it. */
export function ActionErrorBanner({ message, onRetry, onDismiss }: ActionErrorBannerProps) {
  const t = useT();
  return (
    <Banner
      kind="error"
      actions={
        <>
          {onRetry ? <Button onClick={onRetry}>{t("common.retry")}</Button> : null}
          <Button variant="ghost" aria-label={t("common.dismissError")} onClick={onDismiss}>
            ×
          </Button>
        </>
      }
    >
      {message}
    </Banner>
  );
}
