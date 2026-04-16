import { useEffect } from "react";
import { useProjectStore } from "../stores/projectStore";

export function Toast() {
  const { toast, clearToast } = useProjectStore();

  useEffect(() => {
    if (!toast) return;
    const id = setTimeout(clearToast, 2500);
    return () => clearTimeout(id);
  }, [toast, clearToast]);

  if (!toast) return null;

  const color =
    toast.kind === "error"
      ? "bg-red-500/90 border-red-400"
      : "bg-[var(--accent)] border-[var(--accent-hover)]";

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[100] pointer-events-none">
      <div
        className={`${color} text-white text-sm px-4 py-2 rounded-lg shadow-lg border backdrop-blur-sm`}
      >
        {toast.message}
      </div>
    </div>
  );
}
