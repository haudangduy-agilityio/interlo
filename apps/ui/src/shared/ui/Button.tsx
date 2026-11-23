import { useId, type ComponentPropsWithRef } from "react";
import { useT } from "../i18n";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export interface ButtonProps extends ComponentPropsWithRef<"button"> {
  variant?: ButtonVariant;
  /** `sm` is for a button inside a banner or a row. */
  size?: "sm";
  /** A control with no behaviour yet: disabled, and described as "Not available yet". */
  unavailable?: boolean;
}

/** Always a real button. Icon-only uses must pass aria-label. */
export function Button({
  variant = "secondary",
  size,
  unavailable = false,
  type = "button",
  className,
  ...rest
}: ButtonProps) {
  const t = useT();
  const hintId = useId();
  const classes = ["btn", `btn--${variant}`, size ? `btn--${size}` : "", className ?? ""]
    .filter(Boolean)
    .join(" ");
  if (!unavailable) return <button type={type} className={classes} {...rest} />;
  return (
    <>
      <button
        type={type}
        className={classes}
        {...rest}
        disabled
        aria-describedby={hintId}
        title={t("common.notAvailableYet")}
      />
      <span id={hintId} className="sr-only">
        {t("common.notAvailableYet")}
      </span>
    </>
  );
}
