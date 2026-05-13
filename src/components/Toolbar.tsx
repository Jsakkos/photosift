import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useNavigate } from "react-router-dom";
import { ViewSelector } from "./ViewSelector";
import { ProgressBar } from "./ProgressBar";
import { AiProgressIndicator } from "./AiProgressIndicator";

export function Toolbar() {
  const { currentShoot, displayItems, currentIndex, autoAdvance, toggleAutoAdvance, toggleShortcutHints } =
    useProjectStore();
  const openSettings = useSettingsStore((s) => s.openDialog);
  const navigate = useNavigate();

  if (!currentShoot) return null;

  return (
    <div className="flex flex-col">
      {/* Row 1: Shoot info + controls */}
      <div className="flex items-center justify-between px-4 py-2 bg-bg2 border-b border-white/5 text-sm">
        <div className="flex items-center gap-4">
          <button
            onClick={() => navigate("/shoots")}
            title="Back to shoots"
            aria-label="Back to shoot list"
            className="text-fg-dim hover:text-fg transition-colors"
          >
            &larr; Shoots
          </button>
          <span className="font-medium text-fg">
            {currentShoot.slug}
          </span>
          <span className="text-fg-dim">
            {currentShoot.date}
          </span>
          <span className="text-fg-dim">
            {displayItems.length > 0
              ? `${currentIndex + 1} / ${displayItems.length}`
              : "No images"}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <AiProgressIndicator />
          <button
            onClick={toggleShortcutHints}
            title="Keyboard shortcuts (?)"
            className="text-xs text-fg-dim/60 hover:text-fg transition-colors"
          >
            Press ? for shortcuts
          </button>
          <button
            onClick={toggleAutoAdvance}
            title="Toggle auto-advance on pick/reject"
            className={`px-2 py-1 rounded text-xs transition-colors ${
              autoAdvance
                ? "bg-accent-blue text-white"
                : "bg-bg3 text-fg-dim"
            }`}
          >
            Auto-advance {autoAdvance ? "ON" : "OFF"}
          </button>
          <button
            onClick={openSettings}
            title="Settings (,)"
            aria-label="Settings"
            className="w-7 h-7 flex items-center justify-center rounded bg-bg3 text-fg-dim hover:text-fg hover:bg-white/10 transition-colors"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
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
