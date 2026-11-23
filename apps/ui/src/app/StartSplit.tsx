import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { MeetingMode } from "../ipc/types";
import { useT, type MessageKey } from "../shared/i18n";
import { Button, Icon } from "../shared/ui";

const MODES: readonly { mode: MeetingMode; labelKey: MessageKey }[] = [
  { mode: "both", labelKey: "start.mode.both" },
  { mode: "listen", labelKey: "start.menu.listen" },
  { mode: "speak", labelKey: "start.menu.speak" },
];

export interface StartSplitProps {
  onStart: (mode?: MeetingMode) => void;
  onReviewSetup: () => void;
  /** A start request is waiting. */
  busy: boolean;
}

/** The split Start button: Start with both directions, and a menu with the other modes. */
export function StartSplit({ onStart, onReviewSetup, busy }: StartSplitProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const items = () => [...(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];

  useEffect(() => {
    if (open) items()[0]?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!menu.current?.contains(target) && !toggle.current?.contains(target)) setOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [open]);

  const close = (restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) toggle.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLElement);
    let next: number;
    switch (event.code) {
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        close(true);
        return;
      case "ArrowDown":
        next = (at + 1) % all.length;
        break;
      case "ArrowUp":
        next = (at - 1 + all.length) % all.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = all.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    all[next]?.focus();
  };

  return (
    <div className="split">
      <Button variant="primary" disabled={busy} onClick={() => onStart()}>
        <Icon name="play" size={14} />
        {t("live.start")}
      </Button>
      <Button
        ref={toggle}
        variant="primary"
        aria-label={t("topbar.startOptions")}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <Icon name="chev" size={14} />
      </Button>
      {open ? (
        <div
          className="dd"
          role="menu"
          aria-label={t("start.menu.label")}
          ref={menu}
          onKeyDown={onKeyDown}
        >
          {MODES.map(({ mode, labelKey }) => (
            <button
              key={mode}
              type="button"
              role="menuitem"
              onClick={() => {
                close(false);
                onStart(mode);
              }}
            >
              {t(labelKey)}
              {mode === "both" ? <span className="mono">{t("start.menu.default")}</span> : null}
            </button>
          ))}
          <hr />
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              close(false);
              onReviewSetup();
            }}
          >
            {t("start.menu.review")}
          </button>
        </div>
      ) : null}
    </div>
  );
}
