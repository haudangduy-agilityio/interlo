import { useId, type ReactElement } from "react";
import { useT } from "../i18n";

/**
 * For a control with no behaviour yet that is not a `Button` (a search box, an icon button): the
 * attributes to spread on it, and the hidden sentence they point at. Render `hint` next to it.
 */
export function useUnavailable(): {
  attrs: { disabled: true; "aria-describedby": string; title: string };
  hint: ReactElement;
} {
  const t = useT();
  const id = useId();
  return {
    attrs: { disabled: true, "aria-describedby": id, title: t("common.notAvailableYet") },
    hint: (
      <span id={id} className="sr-only">
        {t("common.notAvailableYet")}
      </span>
    ),
  };
}
