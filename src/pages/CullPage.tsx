import { useEffect, useRef } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useProjectStore } from "../stores/projectStore";
import { useKeyboardNav } from "../hooks/useKeyboardNav";
import { Toolbar } from "../components/Toolbar";
import { GridView } from "../components/GridView";
import { EmptyViewState } from "../components/EmptyViewState";
import { TriageShell } from "../components/triage/TriageShell";
import { SelectShell } from "../components/select/SelectShell";
import { RouteShell } from "../components/route/RouteShell";
import { ShortcutsOverlay } from "../components/ShortcutsOverlay";

export function CullPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { currentShoot, isLoading, loadError, loadShoot, viewMode, currentView } = useProjectStore();
  const displayCount = useProjectStore((s) => s.displayItems.length);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  useKeyboardNav();

  // Focus the shell container whenever the shoot opens or the active
  // photo changes. The keyboard listener is window-level (see
  // useKeyboardNav), but after react-router navigation focus often lives
  // on `body`, which on some platforms swallows the first keystroke
  // until an element receives focus. Making the main shell focusable
  // (tabIndex=-1) and calling .focus() on mount removes the "click
  // before keys work" priming step.
  const shellRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (isLoading || !currentShoot) return;
    const raf = requestAnimationFrame(() => {
      shellRef.current?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(raf);
  }, [isLoading, currentShoot?.id, currentIndex]);

  // Tauri window refocus (Alt-Tab back, minimize→restore) doesn't
  // automatically re-home DOM focus to our container. We listen on
  // multiple channels because no single one is reliable on Windows:
  //   - Tauri's onFocusChanged (works for true window focus events)
  //   - DOM `focus` on window (fires for in-webview focus changes)
  //   - `visibilitychange` (covers tab-restore from a minimized state
  //     where the focus event sometimes never fires)
  // All three converge on shellRef.focus(). Skip the refocus when the
  // active element is a real input — otherwise re-focusing the shell
  // would yank focus out of the Settings dialog mid-typing.
  useEffect(() => {
    const refocus = () => {
      const ae = document.activeElement;
      if (
        ae instanceof HTMLInputElement ||
        ae instanceof HTMLTextAreaElement ||
        ae instanceof HTMLSelectElement
      ) {
        return;
      }
      shellRef.current?.focus({ preventScroll: true });
    };

    let unlistenTauri: (() => void) | null = null;
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) refocus();
      })
      .then((fn) => {
        unlistenTauri = fn;
      })
      .catch(() => {});

    const onWinFocus = () => refocus();
    const onVis = () => {
      if (document.visibilityState === "visible") refocus();
    };
    window.addEventListener("focus", onWinFocus);
    document.addEventListener("visibilitychange", onVis);

    return () => {
      unlistenTauri?.();
      window.removeEventListener("focus", onWinFocus);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);

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

    // Curator events: refresh judgments after each cluster lands so
    // chips, the AI-rejects filter, and the cluster-rank ranking
    // update live as the worker progresses.
    let unlistenCuratorCluster: (() => void) | null = null;
    let unlistenCuratorCompleted: (() => void) | null = null;
    const refreshCuratorJudgments =
      useProjectStore.getState().refreshCuratorJudgments;
    listen<{ shootId: number }>("curator:cluster_done", (event) => {
      if (event.payload.shootId === shootId) {
        void refreshCuratorJudgments();
      }
    }).then((fn) => { unlistenCuratorCluster = fn; });
    listen<{ shootId: number }>("curator:completed", (event) => {
      if (event.payload.shootId === shootId) {
        void refreshCuratorJudgments();
      }
    }).then((fn) => { unlistenCuratorCompleted = fn; });

    return () => {
      unlistenReady?.();
      unlistenGroups?.();
      unlistenComplete?.();
      unlistenCuratorCluster?.();
      unlistenCuratorCompleted?.();
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
      ref={shellRef}
      tabIndex={-1}
      className="h-screen w-screen flex flex-col outline-none"
      style={{ background: "var(--color-bg)" }}
    >
      <Toolbar />
      {viewMode === "grid" ? (
        displayCount === 0 ? (
          <EmptyViewState view={currentView} />
        ) : (
          <GridView />
        )
      ) : displayCount === 0 ? (
        <EmptyViewState view={currentView} />
      ) : currentView === "triage" ? (
        <TriageShell />
      ) : currentView === "select" ? (
        <SelectShell />
      ) : (
        <RouteShell />
      )}
      <ShortcutsOverlay />
    </div>
  );
}
