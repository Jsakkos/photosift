import { useProjectStore } from "../stores/projectStore";

const NAV_SHORTCUTS = [
  { key: "← →", action: "Previous / Next image" },
  { key: "Home / End", action: "First / Last image" },
  { key: "Space", action: "Next unreviewed" },
  { key: "Z", action: "Toggle zoom" },
  { key: "I", action: "Toggle metadata" },
  { key: "G", action: "Toggle grid view" },
  { key: ",", action: "Open settings" },
  { key: "Ctrl+E", action: "Export XMP sidecars" },
  { key: "?", action: "Toggle this overlay" },
  { key: "Ctrl+Z", action: "Undo" },
  { key: "Ctrl+Shift+Z", action: "Redo" },
];

const TRIAGE_SHORTCUTS = [
  { key: "P", action: "Pick" },
  { key: "X", action: "Reject" },
  { key: "U", action: "Reset to unreviewed" },
  { key: "A", action: "Toggle auto-advance" },
  { key: "1-5 / 0", action: "Star rating / Clear" },
];

const SELECT_SHORTCUTS = [
  { key: "P", action: "Pick (auto-reject group)" },
  { key: "Shift+P", action: "Pick (keep group)" },
  { key: "X", action: "Reject" },
  { key: "U", action: "Reset to unreviewed" },
  { key: "Tab", action: "2-up comparison" },
  { key: "C", action: "Set group cover" },
  { key: "Ctrl+G", action: "Group selected (grid view)" },
  { key: "Ctrl+Shift+G", action: "Ungroup selected (grid view)" },
];

const ROUTE_SHORTCUTS = [
  { key: "E", action: "Mark for edit" },
  { key: "D", action: "Publish direct" },
  { key: "U", action: "Reset to unrouted" },
];

const COMPARISON_SHORTCUTS = [
  { key: "Tab", action: "Enter comparison" },
  { key: "Shift+Tab / Esc", action: "Exit comparison" },
  { key: "1 / 2", action: "Pick left / right (reject other)" },
  { key: "← →", action: "Cycle right panel" },
];

export function ShortcutHints() {
  const { showShortcutHints, toggleShortcutHints, currentView } =
    useProjectStore();
  if (!showShortcutHints) return null;

  const viewShortcuts =
    currentView === "triage"
      ? TRIAGE_SHORTCUTS
      : currentView === "select"
        ? SELECT_SHORTCUTS
        : ROUTE_SHORTCUTS;

  const viewLabel =
    currentView === "triage"
      ? "Triage"
      : currentView === "select"
        ? "Select"
        : "Route";

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
          {NAV_SHORTCUTS.map(({ key, action }) => (
            <div key={key} className="flex justify-between gap-8 text-sm">
              <kbd className="px-2 py-0.5 rounded bg-[var(--bg-tertiary)] text-[var(--text-primary)] font-mono text-xs">
                {key}
              </kbd>
              <span className="text-[var(--text-secondary)]">{action}</span>
            </div>
          ))}
        </div>
        <h3 className="text-sm font-medium text-[var(--accent)] mt-4 mb-2">
          {viewLabel} Pass
        </h3>
        <div className="space-y-2">
          {viewShortcuts.map(({ key, action }) => (
            <div key={key} className="flex justify-between gap-8 text-sm">
              <kbd className="px-2 py-0.5 rounded bg-[var(--bg-tertiary)] text-[var(--text-primary)] font-mono text-xs">
                {key}
              </kbd>
              <span className="text-[var(--text-secondary)]">{action}</span>
            </div>
          ))}
        </div>
        {currentView === "select" && (
          <>
            <h3 className="text-sm font-medium text-[var(--accent)] mt-4 mb-2">
              Comparison
            </h3>
            <div className="space-y-2">
              {COMPARISON_SHORTCUTS.map(({ key, action }) => (
                <div key={key} className="flex justify-between gap-8 text-sm">
                  <kbd className="px-2 py-0.5 rounded bg-[var(--bg-tertiary)] text-[var(--text-primary)] font-mono text-xs">
                    {key}
                  </kbd>
                  <span className="text-[var(--text-secondary)]">{action}</span>
                </div>
              ))}
            </div>
          </>
        )}
        <p className="mt-4 text-xs text-[var(--text-secondary)] text-center">
          Press ? or click to close
        </p>
      </div>
    </div>
  );
}
