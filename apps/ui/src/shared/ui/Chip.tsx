import type { ReactNode } from "react";

export type ChipKind = "live" | "ok" | "warning" | "error" | "acc";

export interface ChipProps {
  kind?: ChipKind;
  /** Show a dot before the text. A live chip pulses it. The word still carries the state. */
  dot?: boolean;
  children: ReactNode;
}

/** A small status pill. State is also in the word, never colour alone (UA-4). */
export function Chip({ kind, dot = false, children }: ChipProps) {
  return (
    <span className={kind ? `chip chip--${kind}` : "chip"}>
      {dot ? (
        <span className={kind === "live" ? "dot dot--live" : "dot"} aria-hidden="true" />
      ) : null}
      {children}
    </span>
  );
}
