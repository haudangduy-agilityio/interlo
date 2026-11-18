import { useEffect, useRef } from "react";
import { SettingsScreen } from "../features/settings";
import { useT } from "../shared/i18n";
import { Button } from "../shared/ui";
import { useFocusTrap } from "../shared/useFocusTrap";

/**
 * Modal sheet that holds the settings tabs. Escape or the close button closes it. Tab stays
 * inside the sheet; the shell marks the page behind it `inert`.
 */
export function SettingsSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  const ref = useRef<HTMLElement>(null);
  const t = useT();
  const trapTab = useFocusTrap(ref);

  useEffect(() => {
    if (!open) return;
    // Give focus back to the control that opened the sheet, so keyboard users keep their place.
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ref.current?.focus();
    return () => opener?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div className="sheet-backdrop">
      <section
        ref={ref}
        tabIndex={-1}
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onKeyDown={trapTab}
      >
        <header className="sheet__head">
          <h2 id="settings-title">{t("settings.title")}</h2>
          <Button variant="ghost" aria-label={t("settings.close")} onClick={onClose}>
            ×
          </Button>
        </header>
        <SettingsScreen />
      </section>
    </div>
  );
}
