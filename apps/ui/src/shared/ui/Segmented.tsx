import type { KeyboardEvent } from "react";

export interface SegmentedOption<Id extends string> {
  id: Id;
  label: string;
}

export interface SegmentedProps<Id extends string> {
  label: string;
  options: readonly SegmentedOption<Id>[];
  value: Id;
  onChange: (id: Id) => void;
}

/**
 * A segmented control. Each segment is a button with aria-pressed (design system Segmented
 * README). Only the selected segment is in the Tab order; the arrow keys, Home and End move to a
 * segment and select it.
 */
export function Segmented<Id extends string>({
  label,
  options,
  value,
  onChange,
}: SegmentedProps<Id>) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = options.findIndex((o) => o.id === value);
    let next: number;
    switch (event.code) {
      case "ArrowRight":
        next = (current + 1) % options.length;
        break;
      case "ArrowLeft":
        next = (current - 1 + options.length) % options.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = options.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    onChange(options[next].id);
    event.currentTarget.querySelectorAll<HTMLElement>("button")[next]?.focus();
  };
  return (
    <div className="seg" role="group" aria-label={label} onKeyDown={onKeyDown}>
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          aria-pressed={o.id === value}
          tabIndex={o.id === value ? 0 : -1}
          onClick={() => onChange(o.id)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
