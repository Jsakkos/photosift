import { useEffect } from "react";
import { useProjectStore } from "../stores/projectStore";

const INFO_DWELL_MS = 2500;
const ERROR_DWELL_MS = 10000;

export function Toast() {
  const { toast, clearToast } = useProjectStore();

  useEffect(() => {
    if (!toast) return;
    // Sticky variant — toasts with an action button stay until the user
    // dismisses (X) or invokes the action. Used by the Select advance
    // prompt where auto-hide would defeat the point.
    if (toast.action) return;
    const dwell = toast.kind === "error" ? ERROR_DWELL_MS : INFO_DWELL_MS;
    const id = setTimeout(clearToast, dwell);
    return () => clearTimeout(id);
  }, [toast, clearToast]);

  if (!toast) return null;

  const isError = toast.kind === "error";
  const isSticky = toast.action != null;
  const color = isError
    ? "bg-red-500/90 border-red-400"
    : "bg-[var(--accent)] border-[var(--accent-hover)]";

  const handleAction = () => {
    toast.action?.onClick();
    clearToast();
  };

  return (
    <div
      className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[100] pointer-events-none"
      role={isError ? "alert" : "status"}
      aria-live={isError ? "assertive" : "polite"}
    >
      <div
        className={`${color} text-white text-sm px-4 py-2 rounded-lg shadow-lg border backdrop-blur-sm flex items-center gap-3 pointer-events-auto`}
      >
        <span>{toast.message}</span>
        {toast.action && (
          <button
            type="button"
            onClick={handleAction}
            className="bg-white/15 hover:bg-white/25 text-white text-xs font-medium px-2.5 py-1 rounded-md focus-visible:outline-2 focus-visible:outline-white/80"
          >
            {toast.action.label}
          </button>
        )}
        {(isError || isSticky) && (
          <button
            type="button"
            onClick={clearToast}
            aria-label="Dismiss notification"
            className="text-white/80 hover:text-white text-lg leading-none px-1 focus-visible:outline-2 focus-visible:outline-white/80"
          >
            ×
          </button>
        )}
      </div>
    </div>
  );
}
