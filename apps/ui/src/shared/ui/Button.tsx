import type { ButtonHTMLAttributes } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
}

/** Always a real button. Icon-only uses must pass aria-label. */
export function Button({
  variant = "secondary",
  type = "button",
  className,
  ...rest
}: ButtonProps) {
  const cls = className ? `btn btn--${variant} ${className}` : `btn btn--${variant}`;
  return <button type={type} className={cls} {...rest} />;
}
