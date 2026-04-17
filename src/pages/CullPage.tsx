import { useEffect } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useProjectStore } from "../stores/projectStore";
import { useKeyboardNav } from "../hooks/useKeyboardNav";
import { LoupeView } from "../components/LoupeView";
import { Filmstrip } from "../components/Filmstrip";
import { Toolbar } from "../components/Toolbar";
import { RatingBar } from "../components/RatingBar";
import { MetadataOverlay } from "../components/MetadataOverlay";
import { ShortcutHints } from "../components/ShortcutHints";
import { GroupStrip } from "../components/GroupStrip";
import { GridView } from "../components/GridView";
import { ComparisonView } from "../components/ComparisonView";
import { AiPanel } from "../components/AiPanel";

function AiPanelHost() {
  const currentItem = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  const aiPanelForced = useProjectStore((s) => s.aiPanelForced);
  const faceCount = currentItem?.image.faceCount ?? 0;
  const visible = faceCount > 0 || aiPanelForced;
  if (!currentItem) return null;
  return <AiPanel photoId={currentItem.image.id} visible={visible} />;
}

export function CullPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { currentShoot, isLoading, loadError, loadShoot, viewMode } = useProjectStore();
  useKeyboardNav();

  useEffect(() => {
    const shootId = Number(id);
    if (!isNaN(shootId) && shootId > 0) {
      loadShoot(shootId);
    } else {
      navigate("/shoots", { replace: true });
    }
  }, [id, loadShoot, navigate]);

  if (isLoading) {
    return (
      <div className="h-screen w-screen flex items-center justify-center bg-[var(--bg-primary)]">
        <p className="text-[var(--text-secondary)]">Loading shoot...</p>
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="h-screen w-screen flex flex-col items-center justify-center gap-4 bg-[var(--bg-primary)]">
        <p className="text-red-400 font-medium">Could not load shoot</p>
        <p className="text-[var(--text-secondary)] text-sm max-w-md text-center">
          {loadError}
        </p>
        <button
          type="button"
          onClick={() => navigate("/shoots")}
          className="px-4 py-2 rounded bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white text-sm"
        >
          Back to shoots
        </button>
      </div>
    );
  }

  if (!currentShoot) {
    return (
      <div className="h-screen w-screen flex items-center justify-center bg-[var(--bg-primary)]">
        <p className="text-[var(--text-secondary)]">Shoot not found</p>
      </div>
    );
  }

  return (
    <div className="h-screen w-screen flex flex-col bg-[var(--bg-primary)]">
      <Toolbar />
      {viewMode === "grid" ? (
        <GridView />
      ) : viewMode === "comparison" ? (
        <ComparisonView />
      ) : (
        <>
          <div className="flex-1 relative overflow-hidden">
            <LoupeView />
            <MetadataOverlay />
            <AiPanelHost />
            <ShortcutHints />
          </div>
          <GroupStrip />
          <Filmstrip />
          <RatingBar />
        </>
      )}
    </div>
  );
}
