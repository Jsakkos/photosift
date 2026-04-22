import type { CullView } from "../../types";

export type TabId = "library" | CullView;

type Tab = { id: TabId; label: string; number: number };

const TABS: readonly Tab[] = [
  { id: "library", label: "Library", number: 1 },
  { id: "triage", label: "Triage", number: 2 },
  { id: "select", label: "Select", number: 3 },
  { id: "route", label: "Route", number: 4 },
] as const;

type TabBarProps = {
  activeTab: TabId;
  onSelect: (tab: TabId) => void;
  projectName?: string | null;
  disabledTabs?: ReadonlySet<TabId>;
};

export function TabBar({ activeTab, onSelect, projectName, disabledTabs }: TabBarProps) {
  return (
    <div
      className="h-10 flex items-center px-4 gap-[2px] border-b shrink-0"
      style={{ background: "var(--color-bg)", borderColor: "var(--color-border)" }}
    >
      {TABS.map((t) => {
        const active = t.id === activeTab;
        const disabled = disabledTabs?.has(t.id) ?? false;
        return (
          <button
            key={t.id}
            type="button"
            disabled={disabled}
            tabIndex={-1}
            onClick={() => onSelect(t.id)}
            className="relative flex items-center gap-2 px-[14px] py-[8px] text-[12px] font-medium bg-transparent border-0 cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
            style={{
              color: active ? "var(--color-fg)" : "var(--color-fg-dim)",
              borderBottom: active ? "2px solid var(--color-accent-blue)" : "2px solid transparent",
              marginBottom: -1,
            }}
            aria-pressed={active}
          >
            <span className="font-mono text-[10px]" style={{ color: "var(--color-fg-mute)" }}>
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
