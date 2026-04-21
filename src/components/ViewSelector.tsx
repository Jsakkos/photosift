import { useProjectStore } from "../stores/projectStore";
import type { CullView } from "../types";

const VIEWS: { key: CullView; label: string; tooltip: string }[] = [
  { key: "triage", label: "Triage", tooltip: "Triage — pick or reject each photo" },
  { key: "select", label: "Select", tooltip: "Select — choose the best of similar shots" },
  { key: "route", label: "Route", tooltip: "Route — mark for edit or publish" },
];

function useViewStats() {
  const images = useProjectStore((s) => s.images);
  const picks = images.filter((i) => i.flag === "pick").length;
  const rejects = images.filter((i) => i.flag === "reject").length;
  const reviewed = picks + rejects;
  const total = images.length;
  const remaining = total - rejects;
  const editCount = images.filter((i) => i.destination === "edit").length;
  const publishCount = images.filter(
    (i) => i.destination === "publish_direct",
  ).length;
  const unrouted = picks - editCount - publishCount;
  return { picks, rejects, reviewed, total, remaining, editCount, publishCount, unrouted };
}

export function ViewSelector() {
  const currentView = useProjectStore((s) => s.currentView);
  const setView = useProjectStore((s) => s.setView);
  const showReviewed = useProjectStore((s) => s.showReviewed);
  const toggleShowReviewed = useProjectStore((s) => s.toggleShowReviewed);
  const stats = useViewStats();

  let statsText = "";
  switch (currentView) {
    case "triage":
      statsText = `${stats.reviewed}/${stats.total} reviewed · ${stats.picks}✓ · ${stats.rejects}✗`;
      break;
    case "select":
      statsText = `${stats.remaining} remaining · ${stats.picks} picks`;
      break;
    case "route":
      statsText = `${stats.editCount} → Edit · ${stats.publishCount} → Publish Direct · ${stats.unrouted} unrouted`;
      break;
  }

  return (
    <div className="flex items-center justify-between px-4 bg-[#111] border-b border-white/10">
      <div className="flex">
        {VIEWS.map(({ key, label, tooltip }) => (
          <button
            key={key}
            onClick={() => setView(key)}
            title={tooltip}
            aria-current={currentView === key ? "page" : undefined}
            className={`px-5 py-2.5 text-[13px] border-b-2 transition-colors ${
              currentView === key
                ? "text-[var(--accent)] font-semibold border-[var(--accent)]"
                : "text-[var(--text-secondary)] border-transparent hover:text-[var(--text-primary)]"
            }`}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={toggleShowReviewed}
          aria-pressed={showReviewed}
          title={
            showReviewed
              ? "Showing all photos including picked/rejected. Click to hide reviewed again."
              : "Include picked/rejected photos in the list so an accidental P/X can be undone."
          }
          className={`px-2 py-1 rounded text-[11px] font-medium transition-colors ${
            showReviewed
              ? "bg-[var(--accent)] text-white"
              : "bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          }`}
        >
          Show all {showReviewed ? "ON" : "OFF"}
        </button>
        <span className="text-xs text-[var(--text-secondary)]">{statsText}</span>
      </div>
    </div>
  );
}
