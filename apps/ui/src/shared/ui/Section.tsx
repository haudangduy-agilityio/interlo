import { useId, type ReactNode } from "react";

/** A labelled region of a screen. */
export function Section({
  title,
  actions,
  children,
}: {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <section className="panel" aria-labelledby={id}>
      <div className="panel__head">
        <h2 id={id}>{title}</h2>
        {actions ?? null}
      </div>
      {children}
    </section>
  );
}
