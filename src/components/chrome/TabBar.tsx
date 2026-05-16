import { useRef } from "react";
import type { CullView } from "../../types";

export type TabId = "library" | CullView;

type Tab = { id: TabId; label: string; number: number };

const TABS: readonly Tab[] = [
  { id: "library", label: "Library", number: 1 },
  { id: "triage", label: "Triage", number: 2 },
  { id: "select", label: "Select", number: 3 },
  { id: "route", label: "Route", number: 4 },
  { id: "review", label: "Review", number: 5 },
] as const;

type TabBarProps = {
  activeTab: TabId;
  onSelect: (tab: TabId) => void;
  projectName?: string | null;
  disabledTabs?: ReadonlySet<TabId>;
  /// Optional id of the element the tabs control (the routed view's `<main>`).
  /// When supplied, each tab gets `aria-controls={controlsId}` per the WAI-ARIA
  /// tabs pattern; omit when the panel doesn't have a stable element id (e.g.
  /// the primitives demo).
  controlsId?: string;
};

export function TabBar({
  activeTab,
  onSelect,
  projectName,
  disabledTabs,
  controlsId,
}: TabBarProps) {
  const tablistRef = useRef<HTMLDivElement>(null);

  // Roving-tabindex + arrow-key nav per the WAI-ARIA tabs pattern. Only the
  // active tab has `tabIndex=0`; the rest are `-1` and reached via arrow keys
  // within the tablist. Home/End jump to the first/last enabled tab. Disabled
  // tabs are skipped.
  const enabledIndices = TABS.map((t, i) =>
    disabledTabs?.has(t.id) ? -1 : i,
  ).filter((i) => i >= 0);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const active = document.activeElement as HTMLElement | null;
    if (!active || !tablistRef.current?.contains(active)) return;
    const currentIdx = TABS.findIndex(
      (t) => t.id === active.getAttribute("data-tab-id"),
    );
    if (currentIdx < 0) return;
    let nextIdx: number | null = null;
    if (e.key === "ArrowRight") {
      const pos = enabledIndices.indexOf(currentIdx);
      nextIdx = enabledIndices[(pos + 1) % enabledIndices.length];
    } else if (e.key === "ArrowLeft") {
      const pos = enabledIndices.indexOf(currentIdx);
      nextIdx = enabledIndices[(pos - 1 + enabledIndices.length) % enabledIndices.length];
    } else if (e.key === "Home") {
      nextIdx = enabledIndices[0];
    } else if (e.key === "End") {
      nextIdx = enabledIndices[enabledIndices.length - 1];
    }
    if (nextIdx == null) return;
    e.preventDefault();
    const next = tablistRef.current?.querySelector<HTMLButtonElement>(
      `[data-tab-id="${TABS[nextIdx].id}"]`,
    );
    next?.focus();
    onSelect(TABS[nextIdx].id);
  };

  return (
    <div
      ref={tablistRef}
      role="tablist"
      aria-label="App views"
      onKeyDown={handleKeyDown}
      className="h-10 flex items-center px-4 gap-0.5 border-b shrink-0"
      style={{ background: "var(--color-bg)", borderColor: "var(--color-border)" }}
    >
      {TABS.map((t) => {
        const active = t.id === activeTab;
        const disabled = disabledTabs?.has(t.id) ?? false;
        return (
          <button
            key={t.id}
            type="button"
            role="tab"
            data-tab-id={t.id}
            disabled={disabled}
            tabIndex={active ? 0 : -1}
            onClick={() => onSelect(t.id)}
            className="relative flex items-center gap-2 px-3.5 py-2 text-xs font-medium bg-transparent border-0 cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 transition-colors duration-fast"
            style={{
              color: active ? "var(--color-fg)" : "var(--color-fg-dim)",
              borderBottom: active ? "2px solid var(--color-accent-blue)" : "2px solid transparent",
              marginBottom: -1,
            }}
            aria-selected={active}
            {...(controlsId ? { "aria-controls": controlsId } : {})}
          >
            <span className="font-mono text-2xs" style={{ color: "var(--color-fg-mute)" }}>
              {t.number}
            </span>
            {t.label}
          </button>
        );
      })}
      <div className="flex-1" />
      {projectName && (
        <span className="font-mono text-[11px]" style={{ color: "var(--color-fg-dim)" }}>
          {projectName}
        </span>
      )}
    </div>
  );
}
