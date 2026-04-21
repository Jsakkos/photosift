import { useEffect } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { useProjectStore } from "../stores/projectStore";
import { useKeyboardNav } from "../hooks/useKeyboardNav";
import { LoupeView } from "../components/LoupeView";
import { Filmstrip } from "../components/Filmstrip";
import { InnerStrip } from "../components/InnerStrip";
import { Toolbar } from "../components/Toolbar";
import { RatingBar } from "../components/RatingBar";
import { MetadataOverlay } from "../components/MetadataOverlay";
import { ShortcutHints } from "../components/ShortcutHints";
import { GroupStrip } from "../components/GroupStrip";
import { GridView } from "../components/GridView";
import { ComparisonView } from "../components/ComparisonView";
import { AiPanel } from "../components/AiPanel";
import { HeatmapOverlay } from "../components/HeatmapOverlay";
import { EmptyViewState } from "../components/EmptyViewState";

function useAiPanelVisibility() {
  const currentItem = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  const aiPanelForced = useProjectStore((s) => s.aiPanelForced);
  const faceCount = currentItem?.image.faceCount ?? 0;
  const visible = currentItem != null && (faceCount > 0 || aiPanelForced);
  return { visible, photoId: currentItem?.image.id };
}

function AiPanelHost() {
  const { visible, photoId } = useAiPanelVisibility();
  if (!photoId) return null;
  return <AiPanel photoId={photoId} visible={visible} />;
}

/// Main loupe row: image area flex-grows, AI panel docks into a right
/// column when visible so face tiles never cover the photo. The width
/// transition keeps the reflow from snapping when `F` is pressed.
function LoupeRow() {
  const { visible } = useAiPanelVisibility();
  return (
    <div className="flex-1 flex overflow-hidden">
      <div className="flex-1 relative overflow-hidden transition-[width] duration-150">
        <LoupeView />
        <HeatmapHost />
        <MetadataOverlay />
        <ShortcutHints />
      </div>
      <div
        className="transition-[width] duration-150 border-l border-white/10 bg-[rgba(20,20,20,0.92)] overflow-hidden"
        style={{ width: visible ? 380 : 0 }}
        aria-hidden={!visible}
      >
        {visible && <AiPanelHost />}
      </div>
    </div>
  );
}

function HeatmapHost() {
  const currentItem = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  if (!currentItem) return null;
  return <HeatmapOverlay photoId={currentItem.image.id} />;
}

export function CullPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { currentShoot, isLoading, loadError, loadShoot, viewMode, currentView } = useProjectStore();
  const displayCount = useProjectStore((s) => s.displayItems.length);
  useKeyboardNav();

  useEffect(() => {
    const shootId = Number(id);
    if (!isNaN(shootId) && shootId > 0) {
      loadShoot(shootId);
    } else {
      navigate("/shoots", { replace: true });
    }
  }, [id, loadShoot, navigate]);

  // If the shoot was opened before import finished clustering, the
  // store snapshot is stale. Three event subscriptions keep it current:
  //   - `import-photo-ready`   → append the single new photo to `images`
  //                              so the filmstrip grows live
  //   - `shoot-groups-updated` → refetch groups after clustering so
  //                              newly-imported photos pick up cluster
  //                              membership
  //   - `import-complete`      → full reload as a safety net in case
  //                              any events were missed
  useEffect(() => {
    const shootId = Number(id);
    if (isNaN(shootId) || shootId <= 0) return;
    const appendImportedPhoto = useProjectStore.getState().appendImportedPhoto;
    const refetchGroups = useProjectStore.getState().refetchGroups;

    let unlistenReady: (() => void) | null = null;
    let unlistenGroups: (() => void) | null = null;
    let unlistenComplete: (() => void) | null = null;

    listen<{ shootId: number; photoId: number }>("import-photo-ready", (event) => {
      if (event.payload.shootId === shootId) {
        appendImportedPhoto(event.payload.photoId);
      }
    }).then((fn) => { unlistenReady = fn; });

    listen<{ shootId: number }>("shoot-groups-updated", (event) => {
      if (event.payload.shootId === shootId) {
        refetchGroups();
      }
    }).then((fn) => { unlistenGroups = fn; });

    listen<{ shootId: number }>("import-complete", (event) => {
      if (event.payload.shootId === shootId) {
        loadShoot(shootId);
      }
    }).then((fn) => { unlistenComplete = fn; });

    return () => {
      unlistenReady?.();
      unlistenGroups?.();
      unlistenComplete?.();
    };
  }, [id, loadShoot]);

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
        displayCount === 0 ? (
          <EmptyViewState view={currentView} />
        ) : (
          <GridView />
        )
      ) : viewMode === "comparison" ? (
        <ComparisonView />
      ) : displayCount === 0 ? (
        <EmptyViewState view={currentView} />
      ) : (
        <div className="flex-1 flex overflow-hidden">
          <Filmstrip />
          <InnerStrip />
          <div className="flex-1 flex flex-col">
            <LoupeRow />
            <GroupStrip />
            {/* Star rating is a Select-pass concept per spec — keep it
                out of Triage (where P/X is the only decision) and Route
                (where stars are a read-only filter gate, not an input). */}
            {currentView === "select" && <RatingBar />}
          </div>
        </div>
      )}
    </div>
  );
}
