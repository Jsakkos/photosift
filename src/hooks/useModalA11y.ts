import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/// Wires the standard modal-dialog keyboard/focus behaviour onto a dialog
/// element: Escape closes it, Tab is trapped inside, and focus returns to
/// whatever was focused before it opened. The caller still adds
/// `role="dialog"`, `aria-modal="true"` and an `aria-label`/`aria-labelledby`
/// to the element it passes here.
///
/// `enabled` should track the modal's open state — components that
/// early-return `null` when closed must still call this hook unconditionally
/// (rules of hooks), so pass `enabled={open}` rather than gating the call.
export function useModalA11y(
  ref: RefObject<HTMLElement | null>,
  onClose: () => void,
  enabled = true,
): void {
  // Keep the latest onClose without making it a dep — callers often pass an
  // inline arrow, and re-running the effect on every render would re-steal
  // focus into the dialog.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!enabled) return;
    const root = ref.current;
    const previouslyFocused = document.activeElement as HTMLElement | null;

    const focusable = (): HTMLElement[] =>
      root
        ? Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
            (el) => el.offsetParent !== null,
          )
        : [];

    const first = focusable()[0];
    if (first) {
      first.focus();
    } else if (root) {
      root.setAttribute("tabindex", "-1");
      root.focus();
    }

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onCloseRef.current();
        return;
      }
      if (e.key === "Tab" && root) {
        const items = focusable();
        if (items.length === 0) {
          e.preventDefault();
          return;
        }
        const firstEl = items[0];
        const lastEl = items[items.length - 1];
        const active = document.activeElement;
        const inside = root.contains(active);
        if (e.shiftKey && (active === firstEl || !inside)) {
          e.preventDefault();
          lastEl.focus();
        } else if (!e.shiftKey && (active === lastEl || !inside)) {
          e.preventDefault();
          firstEl.focus();
        }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      previouslyFocused?.focus?.();
    };
    // ref is a stable object; only `enabled` should re-run the effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);
}
