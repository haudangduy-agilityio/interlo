import { useT } from "../i18n";
import { Button } from "./Button";

export interface ErrorStateProps {
  message: string;
  onRetry?: () => void;
}

export function ErrorState({ message, onRetry }: ErrorStateProps) {
  const t = useT();
  return (
    <div className="error-state" role="alert">
      <p>{message}</p>
      {onRetry ? <Button onClick={onRetry}>{t("common.retry")}</Button> : null}
    </div>
  );
}
