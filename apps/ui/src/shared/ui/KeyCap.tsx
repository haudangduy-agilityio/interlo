import type { ReactNode } from "react";

/** A keyboard shortcut cap. `sm` is the small cap of the sidebar search. */
export function KeyCap({ size, children }: { size?: "sm"; children: ReactNode }) {
  return <kbd className={size === "sm" ? "key key--sm" : "key"}>{children}</kbd>;
}
