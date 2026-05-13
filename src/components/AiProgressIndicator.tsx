import { invoke } from "@tauri-apps/api/core";
import { useAiStore } from "../stores/aiStore";

export function AiProgressIndicator() {
  const { analyzed, failed, total, provider } = useAiStore();
  if (provider === "disabled") return null;
  // Hide when idle or queue drained.
  if (total === 0 || analyzed + failed >= total) return null;

  const percent = total > 0 ? Math.round(((analyzed + failed) / total) * 100) : 0;

  return (
    <div
      className="flex items-center gap-2 text-xs text-fg-dim"
      role="status"
      aria-live="polite"
    >
      {/* Pulse is decorative — the numeric readout and aria-live below carry
          the state. Under prefers-reduced-motion the global rule in
          globals.css freezes this to a static dot; that's fine because the
          numeric `{analyzed+failed}/{total}` and the sr-only percent below
          keep conveying progress. */}
      <span
        className="inline-block w-2.5 h-2.5 rounded-full bg-accent-blue animate-pulse"
        aria-hidden="true"
      />
      <span>
        On-device AI: {analyzed + failed}/{total}
        {failed > 0 && (
          <span className="text-yellow-400"> · {failed} skipped</span>
        )}
      </span>
      <button
        type="button"
        onClick={() => {
          invoke("cancel_ai_analysis").catch(() => {});
        }}
        className="underline opacity-70 hover:opacity-100"
        aria-label="Cancel on-device AI analysis"
      >
        cancel
      </button>
      <span className="sr-only">{percent}% complete</span>
    </div>
  );
}
