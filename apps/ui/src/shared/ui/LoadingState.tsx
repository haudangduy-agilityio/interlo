import { useT } from "../i18n";

export function LoadingState({ label }: { label?: string }) {
  const t = useT();
  return (
    <p className="loading-state muted" role="status">
      {label ?? t("common.loading")}…
    </p>
  );
}
