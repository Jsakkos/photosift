// Diagnostic logger for focus-channel events. Inert unless the
// VITE_DEBUG_FOCUS env flag is set at build time. Used by CullPage's
// alt-tab refocus useEffect to confirm which channel fires (Tauri's
// onFocusChanged, window focus, visibilitychange, or the pointerdown
// safety net) and what document.activeElement looked like before/after.
// See issue #5 for the bug this was added to investigate.

const ENABLED = import.meta.env.VITE_DEBUG_FOCUS === "1";

function describeActiveElement(): string {
  const ae = document.activeElement;
  if (!ae) return "null";
  const tag = ae.tagName.toLowerCase();
  const id = ae.id ? `#${ae.id}` : "";
  const classes =
    typeof ae.className === "string" && ae.className.length > 0
      ? `.${ae.className.trim().split(/\s+/).join(".")}`
      : "";
  return `${tag}${id}${classes}`;
}

export function logFocus(message: string): void {
  if (!ENABLED) return;
  // eslint-disable-next-line no-console
  console.log(
    `[focus] ${message} ae=${describeActiveElement()} t=${performance.now().toFixed(1)}`,
  );
}

export function isFocusDebugEnabled(): boolean {
  return ENABLED;
}
