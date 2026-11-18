import { useT } from "../i18n";
import { EmptyState } from "./EmptyState";

/** A feature the shell has no command for yet. Say what will be here, never a developer note. */
export function NotAvailable({ children }: { children: string }) {
  const t = useT();
  return <EmptyState title={t("common.notAvailableYet")} description={children} />;
}
