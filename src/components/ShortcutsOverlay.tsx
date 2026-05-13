import { useRef } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useModalA11y } from "../hooks/useModalA11y";
import { Kbd } from "./primitives";

// Source of truth for key bindings lives in src/hooks/useKeyboardNav.ts.
// Keep this table in sync by hand when you add or rename a shortcut.
type Row = { keys: string[]; label: string };
type Section = { heading: string; rows: Row[] };

const SECTIONS: Section[] = [
  {
    heading: "Navigation",
    rows: [
      { keys: ["â†", "â†’"], label: "Previous / next photo" },
      { keys: ["â†‘", "â†“"], label: "Previous / next group" },
      { keys: ["Home"], label: "Jump to first" },
      { keys: ["End"], label: "Jump to last" },
      { keys: ["Space"], label: "Advance to next unreviewed" },
      { keys: ["G"], label: "Toggle grid view" },
      { keys: ["Enter"], label: "Drill into group (from cover)" },
      { keys: ["Esc"], label: "Exit drilled group" },
    ],
  },
  {
    heading: "Triage",
    rows: [
      { keys: ["P"], label: "Keep (pick)" },
      { keys: ["Shift", "P"], label: "Keep everything in current group" },
      { keys: ["X"], label: "Reject" },
      { keys: ["U"], label: "Mark unreviewed" },
      { keys: ["A"], label: "Toggle auto-advance" },
    ],
  },
  {
    heading: "Select",
    rows: [
      { keys: ["1"], label: "Rate â˜…1 (through 5)" },
      { keys: ["0"], label: "Clear rating" },
      { keys: ["["], label: "Lower pass floor" },
      { keys: ["]"], label: "Raise pass floor" },
      { keys: ["Tab"], label: "Enter 2-up comparison" },
      { keys: ["X"], label: "Reject" },
      { keys: ["C"], label: "Set current as group cover" },
      { keys: ["Shift", "A"], label: "Accept AI pick as cover" },
      { keys: ["Alt", "S"], label: "Cycle AI sort" },
    ],
  },
  {
    heading: "Comparison (Tab)",
    rows: [
      { keys: ["â†", "â†’"], label: "Cycle opposite member" },
      { keys: ["1"], label: "Pick left" },
      { keys: ["2"], label: "Pick right" },
      { keys: ["Z"], label: "Toggle zoom" },
      { keys: ["Shift", "Tab"], label: "Exit comparison" },
    ],
  },
  {
    heading: "Panels & overlays",
    rows: [
      { keys: ["F"], label: "Toggle faces rail" },
      { keys: ["T"], label: "Toggle all-strip" },
      { keys: ["H"], label: "Toggle heatmap" },
      { keys: ["Z"], label: "Toggle zoom" },
    ],
  },
  {
    heading: "App",
    rows: [
      { keys: ["?"], label: "Toggle this overlay" },
      { keys: [","], label: "Open settings" },
      { keys: ["Ctrl", "Z"], label: "Undo" },
      { keys: ["Ctrl", "Shift", "Z"], label: "Redo" },
    ],
  },
];

export function ShortcutsOverlay() {
  const show = useProjectStore((s) => s.showShortcutHints);
  const toggle = useProjectStore((s) => s.toggleShortcutHints);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const openWizardTour = useSettingsStore((s) => s.openWizardTour);

  // "Replay tour" â€” re-arm the per-view first-run modals (#13) and
  // close this overlay so the user lands back on the cull view.
  const replayTour = () => {
    void updateSettings({
      onboardedTriage: false,
      onboardedSelect: false,
      onboardedRoute: false,
    });
    toggle();
  };

  // "Take the tour" â€” re-open the onboarding wizard at its three-pass step.
  const takeTheTour = () => {
    openWizardTour();
    toggle();
  };

  // Esc closes the overlay (plus focus-trap / focus-restore) even when
  // focus lives on the main shell.
  const dialogRef = useRef<HTMLDivElement>(null);
  useModalA11y(dialogRef, toggle, show);

  if (!show) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-6"
      style={{ background: "rgba(0,0,0,0.72)" }}
      onClick={toggle}
      role="dialog"
      aria-modal="true"
      aria-label="Keyboard shortcuts"
    >
      <div
        ref={dialogRef}
        className="max-w-[880px] max-h-[85vh] overflow-auto rounded-md"
        style={{
          background: "var(--color-bg2)",
          border: "1px solid var(--color-border)",
          color: "var(--color-fg)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="flex items-center justify-between px-5 py-3 border-b"
          style={{ borderColor: "var(--color-border)" }}
        >
          <div>
            <div
              className="text-3xs uppercase tracking-[1.4px]"
              style={{ color: "var(--color-fg-dim)" }}
            >
              Keyboard
            </div>
            <div className="text-base font-semibold mt-[1px]">
              Shortcuts
            </div>
          </div>
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={takeTheTour}
              className="text-[11px] opacity-70 hover:opacity-100 px-2 py-1 rounded-sm cursor-pointer border-0 bg-transparent underline"
              style={{ color: "var(--color-fg-dim)" }}
              title="Re-open the three-pass onboarding tour"
            >
              Take the tour
            </button>
            <button
              type="button"
              onClick={replayTour}
              className="text-[11px] opacity-70 hover:opacity-100 px-2 py-1 rounded-sm cursor-pointer border-0 bg-transparent underline"
              style={{ color: "var(--color-fg-dim)" }}
              title="Show the per-view first-run guidance again"
            >
              Replay tour
            </button>
            <button
              type="button"
              onClick={toggle}
              className="text-[11px] opacity-70 hover:opacity-100 px-2 py-1 rounded-sm cursor-pointer border-0 bg-transparent"
              style={{ color: "var(--color-fg-dim)" }}
              aria-label="Close shortcuts"
            >
              Close Â· <Kbd>Esc</Kbd>
            </button>
          </div>
        </div>

        <div
          className="grid gap-x-10 gap-y-6 p-5"
          style={{ gridTemplateColumns: "repeat(2, minmax(0, 1fr))" }}
        >
          {SECTIONS.map((section) => (
            <div key={section.heading}>
              <div
                className="text-3xs uppercase tracking-[1.2px] mb-2"
                style={{ color: "var(--color-fg-dim)" }}
              >
                {section.heading}
              </div>
              <div className="flex flex-col gap-1.5">
                {section.rows.map((row, i) => (
                  <div
                    key={i}
                    className="flex items-center justify-between gap-3 text-xs"
                  >
                    <span style={{ color: "var(--color-fg)" }}>
                      {row.label}
                    </span>
                    <span className="flex items-center gap-[3px]">
                      {row.keys.map((k, ki) => (
                        <Kbd key={ki}>{k}</Kbd>
                      ))}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
