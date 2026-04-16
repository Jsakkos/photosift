import { useProjectStore } from "../stores/projectStore";
import { useNavigate } from "react-router-dom";
import { ViewSelector } from "./ViewSelector";
import { ProgressBar } from "./ProgressBar";

export function Toolbar() {
  const { currentShoot, displayItems, currentIndex, autoAdvance, toggleAutoAdvance } =
    useProjectStore();
  const navigate = useNavigate();

  if (!currentShoot) return null;

  return (
    <div className="flex flex-col">
      {/* Row 1: Shoot info + controls */}
      <div className="flex items-center justify-between px-4 py-2 bg-[var(--bg-secondary)] border-b border-white/5 text-sm">
        <div className="flex items-center gap-4">
          <button
            onClick={() => navigate("/shoots")}
            className="text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          >
            &larr; Shoots
          </button>
          <span className="font-medium text-[var(--text-primary)]">
            {currentShoot.slug}
          </span>
          <span className="text-[var(--text-secondary)]">
            {currentShoot.date}
          </span>
          <span className="text-[var(--text-secondary)]">
            {displayItems.length > 0
              ? `${currentIndex + 1} / ${displayItems.length}`
              : "No images"}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={toggleAutoAdvance}
            className={`px-2 py-1 rounded text-xs transition-colors ${
              autoAdvance
                ? "bg-[var(--accent)] text-white"
                : "bg-[var(--bg-tertiary)] text-[var(--text-secondary)]"
            }`}
          >
            Auto-advance {autoAdvance ? "ON" : "OFF"}
          </button>
        </div>
      </div>
      {/* Row 2: View selector + stats */}
      <ViewSelector />
      {/* Progress bar */}
      <ProgressBar />
    </div>
  );
}
