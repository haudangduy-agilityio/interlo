import { useEffect, useId, useRef, type ReactNode } from "react";
import { useT } from "../i18n";
import { useFocusTrap } from "../useFocusTrap";
import { Button } from "./Button";

export interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
}

/**
 * A modal dialog. Focus moves into it on open, stays inside while it is open (Tab wraps), and
 * returns to the opener on close. Escape closes it and stops there, so a dialog inside the
 * settings sheet does not close the sheet.
 */
export function Dialog({ title, onClose, children }: DialogProps) {
  const ref = useRef<HTMLElement>(null);
  const titleId = useId();
  const t = useT();
  const trapTab = useFocusTrap(ref);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ref.current?.focus();
    return () => opener?.focus();
  }, []);

  return (
    <div className="dialog-backdrop">
      <section
        ref={ref}
        tabIndex={-1}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={(event) => {
          trapTab(event);
          if (event.code !== "Escape") return;
          event.stopPropagation();
          onClose();
        }}
      >
        <header className="dialog__head">
          <h2 id={titleId}>{title}</h2>
          <Button variant="ghost" aria-label={t("common.closeDialog")} onClick={onClose}>
            ×
          </Button>
        </header>
        {children}
      </section>
    </div>
  );
}
