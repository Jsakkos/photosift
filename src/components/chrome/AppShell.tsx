import { useNavigate } from "react-router-dom";
import type { ReactNode } from "react";
import { useProjectStore } from "../../stores/projectStore";
import type { CullView } from "../../types";
import { TabBar, type TabId } from "./TabBar";

type AppShellProps = {
  children: ReactNode;
};

export function AppShell({ children }: AppShellProps) {
  const navigate = useNavigate();
  const currentView = useProjectStore((s) => s.currentView);
  const setView = useProjectStore((s) => s.setView);
  const currentShoot = useProjectStore((s) => s.currentShoot);

  const handleSelect = (tab: TabId) => {
    if (tab === "library") {
      navigate("/shoots");
      return;
    }
    void setView(tab as CullView);
  };

  return (
    <div className="h-full flex flex-col" style={{ background: "var(--color-bg)" }}>
      <TabBar
        activeTab={currentView}
        onSelect={handleSelect}
        projectName={currentShoot?.slug ?? null}
      />
      <div className="flex-1 min-h-0 relative">{children}</div>
    </div>
  );
}
