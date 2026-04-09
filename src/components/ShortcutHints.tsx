import { useProjectStore } from "../stores/projectStore";

const SHORTCUTS = [
  { key: "\u2190 \u2192", action: "Previous / Next image" },
  { key: "1-5", action: "Set star rating" },
  { key: "0", action: "Clear rating" },
  { key: "Space", action: "Toggle zoom" },
  { key: "I", action: "Toggle metadata" },
  { key: "?", action: "Toggle this overlay" },
  { key: "Ctrl+Z", action: "Undo" },
  { key: "Ctrl+Shift+Z", action: "Redo" },
  { key: "Ctrl+O", action: "Open folder" },
];

export function ShortcutHints() {
  const { showShortcutHints, toggleShortcutHints } = useProjectStore();
  if (!showShortcutHints) return null;

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={toggleShortcutHints}
    >
      <div className="bg-[var(--bg-secondary)] rounded-lg p-6 shadow-2xl border border-white/10 min-w-80">
        <h2 className="text-lg font-medium text-[var(--text-primary)] mb-4">
          Keyboard Shortcuts
        </h2>
        <div className="space-y-2">
          {SHORTCUTS.map(({ key, action }) => (
            <div key={key} className="flex justify-between gap-8 text-sm">
              <kbd className="px-2 py-0.5 rounded bg-[var(--bg-tertiary)] text-[var(--text-primary)] font-mono text-xs">
                {key}
              </kbd>
              <span className="text-[var(--text-secondary)]">{action}</span>
            </div>
          ))}
        </div>
        <p className="mt-4 text-xs text-[var(--text-secondary)] text-center">
          Press ? or click to close
        </p>
      </div>
    </div>
  );
}
