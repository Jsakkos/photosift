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
import { FirstRunModal } from "../components/FirstRunModal";
import { logFocus } from "../lib/debugFocus";
import { classifyError, formatError } from "../lib/errorMessages";
import {
  resumeCuratorForShoot,
  startCuratorForShoot,
} from "../lib/curatorApi";

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
  // automatically re-home DOM focus to our container. We listen on four
  // channels because no single one is reliable on Windows:
  //   - Tauri's onFocusChanged (true window focus events)
  //   - DOM `focus` on window (in-webview focus changes)
  //   - `visibilitychange` (tab-restore from a minimized state)
  //   - `pointerdown` capture-phase on document (safety net for the
  //     case where the three above all miss — clicking anywhere in the
  //     window restores hotkeys instead of requiring a click into the
  //     cull view itself)
  // All four converge on `refocus`, which:
  //   1) Defers via rAF so the webview has time to settle before we
  //      call .focus() — without this the alt-tab path would call
  //      el.focus() while the webview's keyboard input target is still
  //      the OS-level "lost" state, so the focus call lands but
  //      keystrokes still don't get delivered.
  //   2) Calls getCurrentWindow().setFocus() alongside the DOM focus —
  //      el.focus() alone isn't enough to restore OS-level webview
  //      keyboard input on Windows.
  //   3) Skips the refocus when the active element is a real input —
  //      otherwise re-focusing the shell would yank focus out of the
  //      Settings dialog mid-typing.
  useEffect(() => {
    let rafId: number | null = null;

    const refocus = (channel: string) => {
      logFocus(`${channel} fired`);
      if (rafId !== null) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        rafId = null;
        const ae = document.activeElement;
        if (
          ae instanceof HTMLInputElement ||
          ae instanceof HTMLTextAreaElement ||
          ae instanceof HTMLSelectElement
        ) {
          logFocus(`${channel} skipped — input element focused`);
          return;
        }
        getCurrentWindow().setFocus().catch(() => {});
        shellRef.current?.focus({ preventScroll: true });
        logFocus(`${channel} after refocus`);
      });
    };

    let unlistenTauri: (() => void) | null = null;
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) refocus("tauri:onFocusChanged");
      })
      .then((fn) => {
        unlistenTauri = fn;
      })
      .catch(() => {});

    const onWinFocus = () => refocus("window:focus");
    const onVis = () => {
      if (document.visibilityState === "visible") {
        refocus("document:visibilitychange");
      }
    };
    const onPointerDown = () => refocus("document:pointerdown");

    window.addEventListener("focus", onWinFocus);
    document.addEventListener("visibilitychange", onVis);
    document.addEventListener("pointerdown", onPointerDown, true);

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      unlistenTauri?.();
      window.removeEventListener("focus", onWinFocus);
      document.removeEventListener("visibilitychange", onVis);
      document.removeEventListener("pointerdown", onPointerDown, true);
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
    // update live as the worker progresses. `curator:failed` events
    // also surface here — the worker emits one per stage that errored
    // (Stage 1 schema rejection, network blip, key revoked, etc.) and
    // without a listener every failure was silent (the status bar
    // would just stay "Running" forever).
    let unlistenCuratorCluster: (() => void) | null = null;
    let unlistenCuratorCompleted: (() => void) | null = null;
    let unlistenCuratorFailed: (() => void) | null = null;
    const refreshCuratorJudgments =
      useProjectStore.getState().refreshCuratorJudgments;
    const setToast = useProjectStore.getState().setToast;
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
    listen<{ shootId: number; reason?: string; groupId?: number | null }>(
      "curator:failed",
      (event) => {
        if (event.payload.shootId !== shootId) return;
        const isStage1 = event.payload.groupId == null;
        const rawReason = event.payload.reason ?? "unknown error";
        const category = classifyError(rawReason);
        const friendly = formatError(rawReason);
        // Stage-1 failures take the whole run down; stage-2 is one cluster
        // out of many. Phrase accordingly. Per-cluster failures are noisy
        // enough already that we don't bother offering a retry — the
        // worker keeps going with the next cluster.
        const scope = isStage1 ? "Curator couldn't start" : "Curator skipped a cluster";
        // Category-specific suffix when we can say something useful that
        // the generic friendly message doesn't already cover.
        const detail = (() => {
          switch (category) {
            case "auth":
              return "Check your API key in Settings.";
            case "rate_limit":
            case "server":
              return null; // friendly message already says it; offer retry below
            case "schema":
              return "The provider returned an unexpected response.";
            case "network":
            case "timeout":
              return null; // friendly already says "check your network"
            default:
              return null;
          }
        })();
        const message = detail
          ? `${scope} — ${friendly} ${detail}`
          : `${scope} — ${friendly}`;
        console.error(`[curator] ${isStage1 ? "stage 1" : `cluster ${event.payload.groupId}`} failed: ${rawReason}`);
        // Offer a retry button for transient stage-1 failures (network /
        // timeout / rate limit / server). Auth + schema need user action,
        // so don't offer a one-click retry there.
        const transient =
          isStage1 &&
          (category === "network" ||
            category === "timeout" ||
            category === "rate_limit" ||
            category === "server");
        if (transient) {
          setToast(message, "error", {
            label: "Retry",
            onClick: () => {
              // Prefer resume so judgments already in the DB aren't redone.
              resumeCuratorForShoot(shootId).catch(() =>
                startCuratorForShoot(shootId).catch((e) =>
                  console.error("Curator retry failed:", e),
                ),
              );
            },
          });
        } else {
          setToast(message, "error");
        }
      },
    ).then((fn) => { unlistenCuratorFailed = fn; });

    return () => {
      unlistenReady?.();
      unlistenGroups?.();
      unlistenComplete?.();
      unlistenCuratorCluster?.();
      unlistenCuratorCompleted?.();
      unlistenCuratorFailed?.();
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
          Couldn't load this shoot
        </p>
        <p
          className="text-sm max-w-md text-center"
          style={{ color: "var(--color-fg-dim)" }}
        >
          {formatError(loadError)}
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
      data-testid="cull-page"
      data-view={currentView}
      tabIndex={-1}
      className="h-screen w-screen flex flex-col outline-none"
      style={{ background: "var(--color-bg)" }}
    >
      <Toolbar />
      <main id="cull-main" aria-label="Cull" className="flex-1 min-h-0 flex flex-col">
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
      </main>
      <ShortcutsOverlay />
      <FirstRunModal view={currentView} />
    </div>
  );
}
