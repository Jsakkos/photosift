import { useEffect } from "react";
import { useProjectStore } from "../stores/projectStore";

const INFO_DWELL_MS = 2500;
const ERROR_DWELL_MS = 10000;

export function Toast() {
  const { toast, clearToast } = useProjectStore();

  useEffect(() => {
    if (!toast) return;
    const dwell = toast.kind === "error" ? ERROR_DWELL_MS : INFO_DWELL_MS;
    const id = setTimeout(clearToast, dwell);
    return () => clearTimeout(id);
  }, [toast, clearToast]);

  if (!toast) return null;

  const isError = toast.kind === "error";
  const color = isError
    ? "bg-red-500/90 border-red-400"
    : "bg-[var(--accent)] border-[var(--accent-hover)]";

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
        {isError && (
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
