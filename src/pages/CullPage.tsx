import { useEffect } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { useProjectStore } from "../stores/projectStore";
import { useKeyboardNav } from "../hooks/useKeyboardNav";
import { Toolbar } from "../components/Toolbar";
import { GridView } from "../components/GridView";
import { ComparisonView } from "../components/ComparisonView";
import { EmptyViewState } from "../components/EmptyViewState";
import { TriageShell } from "../components/triage/TriageShell";
import { SelectShell } from "../components/select/SelectShell";
import { RouteShell } from "../components/route/RouteShell";

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
      <div
        className="h-screen w-screen flex items-center justify-center"
        style={{ background: "var(--color-bg)" }}
      >
        <p style={{ color: "var(--color-fg-dim)" }}>Loading shoot…</p>
      </div>
    );
  }

  if (loadError) {
    return (
      <div
        className="h-screen w-screen flex flex-col items-center justify-center gap-4"
        style={{ background: "var(--color-bg)" }}
      >
        <p className="font-medium" style={{ color: "var(--color-danger)" }}>
          Could not load shoot
        </p>
        <p
          className="text-sm max-w-md text-center"
          style={{ color: "var(--color-fg-dim)" }}
        >
          {loadError}
        </p>
        <button
          type="button"
          onClick={() => navigate("/shoots")}
          className="px-4 py-2 rounded-md text-white text-sm cursor-pointer border-0"
          style={{ background: "var(--color-accent-blue)" }}
        >
          Back to shoots
        </button>
      </div>
    );
  }

  if (!currentShoot) {
    return (
      <div
        className="h-screen w-screen flex items-center justify-center"
        style={{ background: "var(--color-bg)" }}
      >
        <p style={{ color: "var(--color-fg-dim)" }}>Shoot not found</p>
      </div>
    );
  }

  return (
    <div
      className="h-screen w-screen flex flex-col"
      style={{ background: "var(--color-bg)" }}
    >
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
      ) : currentView === "triage" ? (
        <TriageShell />
      ) : currentView === "select" ? (
        <SelectShell />
      ) : (
        <RouteShell />
      )}
    </div>
  );
}
