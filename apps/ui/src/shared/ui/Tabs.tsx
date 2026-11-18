import type { KeyboardEvent } from "react";
import { Button } from "./Button";

export interface TabItem<Id extends string> {
  id: Id;
  label: string;
}

export interface TabsProps<Id extends string> {
  label: string;
  tabs: readonly TabItem<Id>[];
  active: Id;
  onChange: (id: Id) => void;
}

/**
 * Tab strip with a roving tabindex: only the active tab is in the Tab order, and the arrow keys
 * (also Home and End) move to another tab and select it. The caller renders the panel with
 * role="tabpanel" and id `panel-<id>`.
 */
export function Tabs<Id extends string>({ label, tabs, active, onChange }: TabsProps<Id>) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = tabs.findIndex((t) => t.id === active);
    let next: number;
    switch (event.code) {
      case "ArrowRight":
        next = (current + 1) % tabs.length;
        break;
      case "ArrowLeft":
        next = (current - 1 + tabs.length) % tabs.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = tabs.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    onChange(tabs[next].id);
    event.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
  };
  return (
    <div className="tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {tabs.map((t) => (
        <Button
          key={t.id}
          variant="ghost"
          role="tab"
          id={`tab-${t.id}`}
          aria-selected={t.id === active}
          aria-controls={`panel-${t.id}`}
          tabIndex={t.id === active ? 0 : -1}
          onClick={() => onChange(t.id)}
        >
          {t.label}
        </Button>
      ))}
    </div>
  );
}
