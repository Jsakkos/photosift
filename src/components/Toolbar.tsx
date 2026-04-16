import { useProjectStore } from "../stores/projectStore";
import { useNavigate } from "react-router-dom";

export function Toolbar() {
  const { currentShoot, images, currentIndex, autoAdvance, toggleAutoAdvance } =
    useProjectStore();
  const navigate = useNavigate();

  if (!currentShoot) return null;

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-[var(--bg-secondary)] border-b border-white/10 text-sm">
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
          {images.length > 0 ? `${currentIndex + 1} / ${images.length}` : "No images"}
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
  );
}
