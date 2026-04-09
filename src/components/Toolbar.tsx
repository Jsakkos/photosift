import { useProjectStore } from "../stores/projectStore";

export function Toolbar() {
  const { projectInfo, images, currentIndex, autoAdvance, toggleAutoAdvance } =
    useProjectStore();

  if (!projectInfo) return null;

  const folderName = projectInfo.folderPath.split(/[/\\]/).pop() || "Unknown";

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-[var(--bg-secondary)] border-b border-white/10 text-sm">
      <div className="flex items-center gap-4">
        <span className="font-medium text-[var(--text-primary)]">{folderName}</span>
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
