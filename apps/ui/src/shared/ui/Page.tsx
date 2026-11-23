import type { ReactNode } from "react";

/** Screen wrapper: one h1, an optional lead line, and the screen's sections. */
export function Page({
  title,
  lead,
  actions,
  children,
}: {
  title: string;
  lead?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="page">
      <header className="page__head">
        <h1>{title}</h1>
        {lead ? <p className="page__lead">{lead}</p> : null}
        {actions ? <div className="page__actions">{actions}</div> : null}
      </header>
      {children}
    </div>
  );
}
