import { useCallback, useEffect, type KeyboardEvent, type RefObject } from "react";

const FOCUSABLE =
  "a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]";

/**
 * Keeps keyboard focus inside a modal region. Tab and Shift+Tab wrap from the last control to
 * the first and back, and focus that lands outside (for example after a click on the backdrop)
 * moves back in. Put the returned handler on the region's `onKeyDown`. It stops the Tab press
 * there, so a modal inside another modal is the only one that handles it.
 */
export function useFocusTrap(
  ref: RefObject<HTMLElement | null>,
): (event: KeyboardEvent<HTMLElement>) => void {
  useEffect(() => {
    const onFocusIn = (event: FocusEvent) => {
      const region = ref.current;
      if (region && event.target instanceof Node && !region.contains(event.target)) region.focus();
    };
    document.addEventListener("focusin", onFocusIn);
    return () => document.removeEventListener("focusin", onFocusIn);
  }, [ref]);

  return useCallback(
    (event) => {
      if (event.code !== "Tab") return;
      const region = ref.current;
      if (!region) return;
      event.stopPropagation();
      const items = Array.from(region.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
        (el) => el.tabIndex >= 0,
      );
      const active = document.activeElement;
      if (items.length === 0) {
        event.preventDefault();
        region.focus();
      } else if (event.shiftKey && (active === items[0] || active === region)) {
        event.preventDefault();
        items[items.length - 1].focus();
      } else if (!event.shiftKey && active === items[items.length - 1]) {
        event.preventDefault();
        items[0].focus();
      }
    },
    [ref],
  );
}
