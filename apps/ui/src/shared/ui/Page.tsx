import type { ReactNode } from "react";

/** Screen wrapper: one h1 and the screen's sections. */
export function Page({
  title,
  actions,
  children,
}: {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="page">
      <header className="page__head">
        <h1>{title}</h1>
        {actions ?? null}
      </header>
      {children}
    </div>
  );
}
