import { useLayoutEffect, useRef } from "react";

export interface MeterProps {
  label: string;
  /** 0 to 100. Leave it out when `unavailable`. */
  value?: number;
  /** The live colour, while recording. */
  live?: boolean;
  /** No measurement yet: an empty track, no value. */
  unavailable?: boolean;
}

/** A 6 px bar on a track. Put the numbers in text beside it. */
export function Meter({ label, value = 0, live = false, unavailable = false }: MeterProps) {
  const percent = Math.min(100, Math.max(0, value));
  const className = live ? "meter meter--live" : "meter";
  const fill = useRef<HTMLElement>(null);
  // The width goes through the element's style object, not an inline attribute: the CSP has no
  // 'unsafe-inline' for styles, and this route is allowed (security_config test).
  useLayoutEffect(() => {
    if (fill.current) fill.current.style.width = `${percent}%`;
  }, [percent]);
  if (unavailable) {
    return (
      <div
        className={className}
        role="meter"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-disabled="true"
      />
    );
  }
  return (
    <div
      className={className}
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent}
    >
      <i ref={fill} />
    </div>
  );
}
