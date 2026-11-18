import type { ReactNode } from "react";

export type BannerKind = "info" | "warning" | "error";

export interface BannerProps {
  kind: BannerKind;
  children: ReactNode;
  /** Optional buttons shown beside the message. */
  actions?: ReactNode;
  /** Optional block under the message, inside the same alert (for example a list of steps). */
  details?: ReactNode;
}

/** A message that stays on screen until the cause is gone. Errors interrupt; others do not. */
export function Banner({ kind, children, actions, details }: BannerProps) {
  const role = kind === "error" ? "alert" : "status";
  if (details) {
    return (
      <div className={`banner banner--${kind} banner--stacked`} role={role}>
        <div className="banner__row">
          <p className="banner__text">{children}</p>
          {actions ? <div className="banner__actions">{actions}</div> : null}
        </div>
        {details}
      </div>
    );
  }
  return (
    <div className={`banner banner--${kind}`} role={role}>
      <p className="banner__text">{children}</p>
      {actions ? <div className="banner__actions">{actions}</div> : null}
    </div>
  );
}
