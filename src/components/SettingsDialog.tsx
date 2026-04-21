import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useSettingsStore } from "../stores/settingsStore";
import { useProjectStore } from "../stores/projectStore";
import { useAiStore } from "../stores/aiStore";
import type { AiProviderStatus } from "../types";

function providerLabel(p: AiProviderStatus): { text: string; color: string } {
  switch (p) {
    case "cuda":
      return { text: "GPU (CUDA)", color: "text-emerald-400" };
    case "cpu":
      return { text: "CPU (CUDA unavailable)", color: "text-amber-400" };
    case "disabled":
      return { text: "Disabled (model load failed)", color: "text-red-400" };
  }
}

export function SettingsDialog() {
  const { isOpen, settings, closeDialog, updateSettings, reclusterShoot } =
    useSettingsStore();
  const { currentShoot, loadShoot, refreshDisplay } = useProjectStore();

  const [nearDup, setNearDup] = useState(settings.nearDupThreshold);
  const [related, setRelated] = useState(settings.relatedThreshold);
  const [timeWindow, setTimeWindow] = useState(settings.groupTimeWindowS);
  const [selectPick, setSelectPick] = useState(settings.selectRequiresPick);
  const [routeStar, setRouteStar] = useState(settings.routeMinStar);
  const [libraryRoot, setLibraryRoot] = useState<string | null>(settings.libraryRoot);
  const [libraryRootError, setLibraryRootError] = useState<string | null>(null);
  const [immichPath, setImmichPath] = useState<string | null>(settings.immichIngestPath);
  const [reclustering, setReclustering] = useState(false);
  const [reclusterMsg, setReclusterMsg] = useState<string | null>(null);
  const [enableAi, setEnableAi] = useState(settings.enableAiOnImport);
  const [eyeConfidence, setEyeConfidence] = useState(settings.eyeOpenConfidence);
  const [reanalyzing, setReanalyzing] = useState(false);
  const [reanalyzeMsg, setReanalyzeMsg] = useState<string | null>(null);
  const aiProvider = useAiStore((s) => s.provider);

  useEffect(() => {
    if (isOpen) {
      setNearDup(settings.nearDupThreshold);
      setRelated(settings.relatedThreshold);
      setTimeWindow(settings.groupTimeWindowS);
      setSelectPick(settings.selectRequiresPick);
      setRouteStar(settings.routeMinStar);
      setLibraryRoot(settings.libraryRoot);
      setImmichPath(settings.immichIngestPath);
      setEnableAi(settings.enableAiOnImport);
      setEyeConfidence(settings.eyeOpenConfidence);
      setLibraryRootError(null);
      setReclusterMsg(null);
      setReanalyzeMsg(null);
    }
  }, [isOpen, settings]);

  const handleBrowseLibraryRoot = useCallback(async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") {
      setLibraryRoot(selected);
      setLibraryRootError(null);
    }
  }, []);

  const handleResetLibraryRoot = useCallback(() => {
    setLibraryRoot(null);
    setLibraryRootError(null);
  }, []);

  const handleBrowseImmichPath = useCallback(async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") setImmichPath(selected);
  }, []);

  const handleResetImmichPath = useCallback(() => setImmichPath(null), []);

  if (!isOpen) return null;

  const valid =
    nearDup >= 0 &&
    nearDup <= 64 &&
    related >= nearDup &&
    related <= 64 &&
    routeStar >= 0 &&
    routeStar <= 5;

  const handleSave = async () => {
    try {
      await updateSettings({
        nearDupThreshold: nearDup,
        relatedThreshold: related,
        groupTimeWindowS: timeWindow,
        selectRequiresPick: selectPick,
        routeMinStar: routeStar,
        libraryRoot,
        immichIngestPath: immichPath,
        enableAiOnImport: enableAi,
        eyeOpenConfidence: eyeConfidence,
      });
    } catch (e) {
      setLibraryRootError(String(e));
      return;
    }
    // Refresh displayItems so the triage-expand toggle takes effect immediately
    // without waiting for the next flag/view change.
    refreshDisplay();
    closeDialog();
  };

  const handleRecluster = async () => {
    if (!currentShoot || !valid) return;
    setReclustering(true);
    setReclusterMsg(null);
    try {
      await updateSettings({
        nearDupThreshold: nearDup,
        relatedThreshold: related,
        groupTimeWindowS: timeWindow,
      });
      const groupCount = await reclusterShoot(currentShoot.id);
      await loadShoot(currentShoot.id);
      setReclusterMsg(`Re-clustered: ${groupCount} group${groupCount === 1 ? "" : "s"}`);
    } catch (e) {
      setReclusterMsg(`Error: ${e}`);
    } finally {
      setReclustering(false);
    }
  };

  const handleReanalyze = async () => {
    if (!currentShoot) return;
    if (!window.confirm("Re-analyze this shoot? Existing AI data will be discarded.")) return;
    setReanalyzing(true);
    setReanalyzeMsg(null);
    try {
      await invoke("reanalyze_shoot", { shootId: currentShoot.id });
      setReanalyzeMsg("Re-analysis queued.");
    } catch (e) {
      setReanalyzeMsg(`Error: ${e}`);
    } finally {
      setReanalyzing(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-[var(--bg-secondary)] rounded-xl border border-white/10 p-6 w-[480px] max-w-[90vw] max-h-[90vh] overflow-y-auto">
        <h2 className="text-xl font-medium text-[var(--text-primary)] mb-4">Settings</h2>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Library root (for copy-mode imports)
          </label>
          <div className="flex gap-2">
            <input
              type="text"
              value={libraryRoot ?? ""}
              readOnly
              placeholder="Default: system Pictures folder"
              className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
            />
            <button
              onClick={handleBrowseLibraryRoot}
              title="Pick a library root directory"
              className="px-3 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-primary)] hover:bg-white/10 transition-colors text-sm"
            >
              Browse
            </button>
            {libraryRoot !== null && (
              <button
                onClick={handleResetLibraryRoot}
                title="Reset to system default"
                className="px-3 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/10 transition-colors text-sm"
              >
                Reset
              </button>
            )}
          </div>
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Copy-mode imports create shoots under <code>{"{root}"}/DSLR/YYYY/YYYY-MM_slug/RAW/</code>. In-place imports ignore this.
          </p>
          {libraryRootError && (
            <p className="text-xs text-red-400 mt-1">{libraryRootError}</p>
          )}
        </div>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Immich ingest folder (for Publish Direct)
          </label>
          <div className="flex gap-2">
            <input
              type="text"
              value={immichPath ?? ""}
              readOnly
              placeholder="Not configured — Publish Direct disabled"
              className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
            />
            <button
              onClick={handleBrowseImmichPath}
              title="Pick the Immich ingest directory"
              className="px-3 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-primary)] hover:bg-white/10 transition-colors text-sm"
            >
              Browse
            </button>
            {immichPath !== null && (
              <button
                onClick={handleResetImmichPath}
                title="Clear (disables Publish Direct)"
                className="px-3 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/10 transition-colors text-sm"
              >
                Reset
              </button>
            )}
          </div>
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Photos flagged with <kbd className="px-1 bg-[var(--bg-tertiary)] rounded">D</kbd> (publish direct) have their cached JPEG copied here. Re-running skips files that already exist.
          </p>
        </div>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Near-duplicate threshold (hamming distance, 0–8 typical)
          </label>
          <input
            type="number"
            min={0}
            max={64}
            value={nearDup}
            onChange={(e) => setNearDup(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
          />
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Lower = stricter. Default 4.
          </p>
        </div>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Related threshold (hamming distance)
          </label>
          <input
            type="number"
            min={0}
            max={64}
            value={related}
            onChange={(e) => setRelated(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
          />
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Must be ≥ near-duplicate threshold. Default 12.
          </p>
        </div>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Group time window (seconds)
          </label>
          <input
            type="number"
            min={0}
            max={3600}
            value={timeWindow}
            onChange={(e) => setTimeWindow(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
          />
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Two photos only cluster if their capture times are within this
            gap AND their pHashes are similar. Blocks cross-moment pHash
            false-positives. Default 60. Set to 0 to disable and use
            pHash-only similarity.
          </p>
        </div>

        <div className="mb-4">
          <label className="flex items-center gap-2 text-sm text-[var(--text-primary)] cursor-pointer">
            <input
              type="checkbox"
              checked={selectPick}
              onChange={(e) => setSelectPick(e.target.checked)}
              className="w-4 h-4"
            />
            Select view requires pick (hide unreviewed)
          </label>
          <p className="text-xs text-[var(--text-secondary)] mt-1 ml-6">
            When on, photos must pass triage before reaching Select. Off = current behavior (shows picks + unreviewed).
          </p>
        </div>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Route minimum star rating (0 = any)
          </label>
          <input
            type="number"
            min={0}
            max={5}
            value={routeStar}
            onChange={(e) => setRouteStar(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
          />
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Route view only shows picks rated ≥ N. Default 3. Set to 0 to disable.
          </p>
        </div>

        {!valid && (
          <p className="text-red-400 text-sm mb-3">
            Invalid values: thresholds within 0–64 (related ≥ near-duplicate), route star 0–5.
          </p>
        )}

        {currentShoot && (
          <div className="mb-4 p-3 rounded-lg bg-[var(--bg-primary)] border border-white/5">
            <div className="flex items-center justify-between">
              <span className="text-sm text-[var(--text-secondary)]">
                Re-cluster current shoot with these thresholds
              </span>
              <button
                onClick={handleRecluster}
                disabled={reclustering || !valid}
                className="px-3 py-1.5 rounded bg-[var(--bg-tertiary)] hover:bg-white/10 text-[var(--text-primary)] text-xs transition-colors disabled:opacity-50"
              >
                {reclustering ? "Re-clustering..." : "Re-cluster"}
              </button>
            </div>
            {reclusterMsg && (
              <p className="text-xs text-[var(--accent)] mt-2">{reclusterMsg}</p>
            )}
          </div>
        )}

        <div className="mb-4 pt-4 border-t border-white/5">
          <h3 className="text-sm font-semibold text-[var(--text-primary)] mb-3">
            AI analysis
          </h3>

          <div className="mb-3 flex items-center justify-between text-sm">
            <span className="text-[var(--text-secondary)]">Inference backend</span>
            <span className={providerLabel(aiProvider).color}>
              {providerLabel(aiProvider).text}
            </span>
          </div>

          <label className="flex items-center gap-2 text-sm text-[var(--text-primary)] cursor-pointer mb-3">
            <input
              type="checkbox"
              checked={enableAi}
              onChange={(e) => setEnableAi(e.target.checked)}
              className="w-4 h-4"
            />
            Enable AI analysis on import
          </label>
          <p className="text-xs text-[var(--text-secondary)] -mt-2 ml-6 mb-3">
            When on, each import kicks off face + eye + sharpness analysis in the background.
          </p>

          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Eye open/closed classifier confidence: {eyeConfidence.toFixed(2)}
          </label>
          <input
            type="range"
            min={0.5}
            max={0.9}
            step={0.05}
            value={eyeConfidence}
            onChange={(e) => setEyeConfidence(parseFloat(e.target.value))}
            className="w-full mb-3"
            aria-label="Eye open/closed classifier confidence"
          />

          {currentShoot && (
            <div className="mt-4 p-3 rounded-lg bg-[var(--bg-primary)] border border-white/5">
              <div className="flex items-center justify-between">
                <span className="text-sm text-[var(--text-secondary)]">
                  Re-analyze this shoot with AI
                </span>
                <button
                  onClick={handleReanalyze}
                  disabled={reanalyzing}
                  className="px-3 py-1.5 rounded bg-[var(--bg-tertiary)] hover:bg-white/10 text-[var(--text-primary)] text-xs transition-colors disabled:opacity-50"
                >
                  {reanalyzing ? "Queuing..." : "Re-analyze"}
                </button>
              </div>
              {reanalyzeMsg && (
                <p className="text-xs text-[var(--accent)] mt-2">{reanalyzeMsg}</p>
              )}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2">
          <button
            onClick={closeDialog}
            className="px-4 py-2 rounded-lg text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!valid}
            className="px-4 py-2 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors disabled:opacity-50"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
