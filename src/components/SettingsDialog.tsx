import { useState, useEffect, useCallback, useRef, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useModalA11y } from "../hooks/useModalA11y";
import { open } from "@tauri-apps/plugin-dialog";
import { Spinner } from "./primitives";
import { formatError } from "../lib/errorMessages";
import { useSettingsStore } from "../stores/settingsStore";
import type { FolderTemplate } from "../stores/settingsStore";
import { useProjectStore } from "../stores/projectStore";
import { useAiStore } from "../stores/aiStore";
import { FolderLayoutEditor } from "./FolderLayoutEditor";
import { validateFolderTemplate } from "../lib/folderTemplate";
import type { AiProviderStatus, ApiKeyStatus, CuratorProvider } from "../types";
import {
  clearCuratorApiKey,
  clearCuratorForShoot,
  estimateCuratorCostCents,
  formatCostCents,
  getCuratorApiKeyStatus,
  setCuratorApiKey,
  startCuratorForShoot,
  testCuratorConnection,
} from "../lib/curatorApi";

/// One-liner feedback for an operation (recluster / reanalyze / connection
/// test / curator run). Errors render in danger red; everything else (in
/// progress, done) in a calm dim. The error heuristic keys off our canonical
/// "Couldn't …" failure prefix plus the legacy "error/failed" wording in case
/// any handler still uses it.
function StatusLine({ msg }: { msg: string | null }) {
  if (!msg) return null;
  const isError = /couldn['’]t|error|failed/i.test(msg);
  return (
    <p className={`text-xs mt-2 break-words ${isError ? "text-danger" : "text-fg-dim"}`}>
      {msg}
    </p>
  );
}

/// Button content for an action that may be running: shows a spinner +
/// the busy label while `busy`, else the idle label.
function BusyLabel({ busy, busyText, children }: { busy: boolean; busyText: string; children: ReactNode }) {
  if (!busy) return <>{children}</>;
  return (
    <span className="inline-flex items-center gap-1.5">
      <Spinner size={11} />
      {busyText}
    </span>
  );
}

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
  const { currentShoot, loadShoot, refreshDisplay, setSelectMinStar } =
    useProjectStore();

  const [threshold, setThreshold] = useState(settings.groupThreshold);
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
  // Curator section. Per-provider state so flipping the dropdown
  // remembers the user's last model + lets each cloud provider keep its
  // own keychain status independent.
  const [curatorRunOnImport, setCuratorRunOnImport] = useState(
    settings.curatorDefaultRunOnImport,
  );
  const [triageOnImport, setTriageOnImport] = useState(
    settings.curatorTriageOnImport,
  );
  const [curatorProvider, setCuratorProvider] = useState<CuratorProvider>(
    settings.curatorProvider,
  );
  const [modelAnthropic, setModelAnthropic] = useState(settings.curatorModelAnthropic);
  const [modelGemini, setModelGemini] = useState(settings.curatorModelGemini);
  const [modelLocal, setModelLocal] = useState(settings.curatorModelLocal);
  const [localBaseUrl, setLocalBaseUrl] = useState(settings.curatorLocalBaseUrl);
  const [curatorMaxCostDollars, setCuratorMaxCostDollars] = useState(
    (settings.curatorMaxCostPerShootCents / 100).toFixed(2),
  );
  const [keyStatusAnthropic, setKeyStatusAnthropic] =
    useState<ApiKeyStatus | null>(null);
  const [keyStatusGemini, setKeyStatusGemini] = useState<ApiKeyStatus | null>(null);
  const [newKey, setNewKey] = useState("");
  const [keyMsg, setKeyMsg] = useState<string | null>(null);
  const [keyBusy, setKeyBusy] = useState(false);
  const [curatorRunning, setCuratorRunning] = useState(false);
  const [curatorMsg, setCuratorMsg] = useState<string | null>(null);
  const [curatorEstimate, setCuratorEstimate] = useState<number | null>(null);
  const [folderTemplate, setFolderTemplate] = useState<FolderTemplate>(
    settings.folderTemplate,
  );

  /// Keychain status for the currently-selected provider. Local has no
  /// key so it always shows as "configured" (no auth needed).
  const activeKeyStatus: ApiKeyStatus | null =
    curatorProvider === "anthropic"
      ? keyStatusAnthropic
      : curatorProvider === "gemini"
        ? keyStatusGemini
        : { configured: true, suffix: "" };

  useEffect(() => {
    if (!isOpen || !currentShoot) {
      setCuratorEstimate(null);
      return;
    }
    let cancelled = false;
    estimateCuratorCostCents(currentShoot.photoCount)
      .then((c) => {
        if (!cancelled) setCuratorEstimate(c);
      })
      .catch(() => {
        if (!cancelled) setCuratorEstimate(null);
      });
    return () => {
      cancelled = true;
    };
  }, [isOpen, currentShoot]);

  useEffect(() => {
    if (isOpen) {
      setThreshold(settings.groupThreshold);
      setTimeWindow(settings.groupTimeWindowS);
      setSelectPick(settings.selectRequiresPick);
      setRouteStar(settings.routeMinStar);
      setLibraryRoot(settings.libraryRoot);
      setImmichPath(settings.immichIngestPath);
      setEnableAi(settings.enableAiOnImport);
      setEyeConfidence(settings.eyeOpenConfidence);
      setCuratorRunOnImport(settings.curatorDefaultRunOnImport);
      setTriageOnImport(settings.curatorTriageOnImport);
      setCuratorProvider(settings.curatorProvider);
      setModelAnthropic(settings.curatorModelAnthropic);
      setModelGemini(settings.curatorModelGemini);
      setModelLocal(settings.curatorModelLocal);
      setLocalBaseUrl(settings.curatorLocalBaseUrl);
      setCuratorMaxCostDollars((settings.curatorMaxCostPerShootCents / 100).toFixed(2));
      setFolderTemplate(settings.folderTemplate);
      setLibraryRootError(null);
      setReclusterMsg(null);
      setReanalyzeMsg(null);
      setNewKey("");
      setKeyMsg(null);
      setCuratorMsg(null);
      // Refresh key status for the cloud providers whenever the dialog
      // opens — the keychain is outside our reactive state, so nothing
      // else triggers this read. Local has no key so we skip it.
      getCuratorApiKeyStatus("anthropic")
        .then(setKeyStatusAnthropic)
        .catch((e) => {
          console.error("Failed to read Anthropic key status:", e);
          setKeyStatusAnthropic({ configured: false, suffix: "" });
        });
      getCuratorApiKeyStatus("gemini")
        .then(setKeyStatusGemini)
        .catch((e) => {
          console.error("Failed to read Gemini key status:", e);
          setKeyStatusGemini({ configured: false, suffix: "" });
        });
    }
  }, [isOpen, settings]);

  // Clear any prior key-action message when the user flips providers so
  // a stale "Couldn't reach the provider" doesn't bleed across providers.
  useEffect(() => {
    setKeyMsg(null);
    setNewKey("");
  }, [curatorProvider]);

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

  const dialogRef = useRef<HTMLDivElement>(null);
  useModalA11y(dialogRef, closeDialog, isOpen);

  if (!isOpen) return null;

  const folderTemplateErrors = validateFolderTemplate(folderTemplate).all;
  const valid =
    threshold >= 0 &&
    threshold <= 64 &&
    routeStar >= 0 &&
    routeStar <= 5 &&
    folderTemplateErrors.length === 0;

  const handleSave = async () => {
    const parsedDollars = parseFloat(curatorMaxCostDollars);
    const maxCents = Number.isFinite(parsedDollars)
      ? Math.max(0, Math.round(parsedDollars * 100))
      : settings.curatorMaxCostPerShootCents;
    try {
      await updateSettings({
        groupThreshold: threshold,
        groupTimeWindowS: timeWindow,
        selectRequiresPick: selectPick,
        routeMinStar: routeStar,
        libraryRoot,
        immichIngestPath: immichPath,
        enableAiOnImport: enableAi,
        eyeOpenConfidence: eyeConfidence,
        curatorDefaultRunOnImport: curatorRunOnImport,
        curatorTriageOnImport: triageOnImport,
        // Keep the legacy single-model field in sync with whichever
        // model the active provider uses, so any code path still
        // reading `curator_model` sees a sensible value.
        curatorModel: activeModel(),
        curatorMaxCostPerShootCents: maxCents,
        curatorProvider,
        curatorModelAnthropic: modelAnthropic,
        curatorModelGemini: modelGemini,
        curatorModelLocal: modelLocal,
        curatorLocalBaseUrl: localBaseUrl,
        folderTemplate,
      });
    } catch (e) {
      setLibraryRootError(String(e));
      return;
    }
    // A lowered routeMinStar shrinks the reachable Select pass floors, so
    // re-clamp the current floor before refreshing (no-op if already in range).
    setSelectMinStar(useProjectStore.getState().selectMinStar);
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
        groupThreshold: threshold,
        groupTimeWindowS: timeWindow,
      });
      const groupCount = await reclusterShoot(currentShoot.id);
      await loadShoot(currentShoot.id);
      setReclusterMsg(
        `Re-clustered into ${groupCount} group${groupCount === 1 ? "" : "s"}`,
      );
    } catch (e) {
      setReclusterMsg(`Couldn't re-cluster — ${formatError(e)}`);
    } finally {
      setReclustering(false);
    }
  };

  const activeModel = (): string => {
    switch (curatorProvider) {
      case "anthropic":
        return modelAnthropic;
      case "gemini":
        return modelGemini;
      case "local":
        return modelLocal;
    }
  };

  const setActiveKeyStatus = (status: ApiKeyStatus | null) => {
    if (curatorProvider === "anthropic") setKeyStatusAnthropic(status);
    else if (curatorProvider === "gemini") setKeyStatusGemini(status);
  };

  const handleSaveKey = async () => {
    if (!newKey.trim()) return;
    if (curatorProvider === "local") return; // no key to save for local
    setKeyBusy(true);
    setKeyMsg(null);
    try {
      await setCuratorApiKey(curatorProvider, newKey.trim());
      const status = await getCuratorApiKeyStatus(curatorProvider);
      setActiveKeyStatus(status);
      setNewKey("");
      setKeyMsg("Key saved.");
    } catch (e) {
      setKeyMsg(`Couldn't save key — ${formatError(e)}`);
    } finally {
      setKeyBusy(false);
    }
  };

  const handleClearKey = async () => {
    if (curatorProvider === "local") return;
    if (!window.confirm(`Remove the stored ${providerName(curatorProvider)} API key?`))
      return;
    setKeyBusy(true);
    setKeyMsg(null);
    try {
      await clearCuratorApiKey(curatorProvider);
      setActiveKeyStatus({ configured: false, suffix: "" });
      setKeyMsg("Key cleared.");
    } catch (e) {
      setKeyMsg(`Couldn't clear key — ${formatError(e)}`);
    } finally {
      setKeyBusy(false);
    }
  };

  /// Persist the current provider + model + base URL before testing,
  /// so the Rust side reads the same values the user is seeing in the
  /// dialog. Without this, "Test" would test whatever was saved before
  /// the user started editing.
  const handleTestConnection = async () => {
    setKeyBusy(true);
    setKeyMsg("Testing…");
    try {
      await updateSettings({
        curatorProvider,
        curatorModelAnthropic: modelAnthropic,
        curatorModelGemini: modelGemini,
        curatorModelLocal: modelLocal,
        curatorLocalBaseUrl: localBaseUrl,
      });
      await testCuratorConnection();
      setKeyMsg("Connection OK.");
    } catch (e) {
      setKeyMsg(`Couldn't reach the provider — ${formatError(e)}`);
    } finally {
      setKeyBusy(false);
    }
  };

  function providerName(p: CuratorProvider): string {
    return p === "anthropic" ? "Anthropic" : p === "gemini" ? "Gemini" : "Local";
  }

  const handleRunCurator = async () => {
    if (!currentShoot) return;
    setCuratorRunning(true);
    setCuratorMsg(null);
    try {
      await startCuratorForShoot(currentShoot.id);
      setCuratorMsg(
        "Started. Watch the Triage chip / 'Curator rejects' filter populate as clusters complete.",
      );
    } catch (e) {
      setCuratorMsg(`Couldn't start the Curator — ${formatError(e)}`);
    } finally {
      setCuratorRunning(false);
    }
  };

  const handleRerunCurator = async () => {
    if (!currentShoot) return;
    if (
      !window.confirm(
        "Re-run the Curator on this shoot? Existing Curator judgments will be discarded.",
      )
    )
      return;
    setCuratorRunning(true);
    setCuratorMsg(null);
    try {
      await clearCuratorForShoot(currentShoot.id);
      await startCuratorForShoot(currentShoot.id);
      setCuratorMsg("Cleared and re-running.");
    } catch (e) {
      setCuratorMsg(`Couldn't re-run the Curator — ${formatError(e)}`);
    } finally {
      setCuratorRunning(false);
    }
  };

  const handleReanalyze = async () => {
    if (!currentShoot) return;
    if (!window.confirm("Re-analyze this shoot with on-device AI? Existing on-device AI data will be discarded.")) return;
    setReanalyzing(true);
    setReanalyzeMsg(null);
    try {
      await invoke("reanalyze_shoot", { shootId: currentShoot.id });
      setReanalyzeMsg("Re-analysis queued.");
    } catch (e) {
      setReanalyzeMsg(`Couldn't re-analyze — ${formatError(e)}`);
    } finally {
      setReanalyzing(false);
    }
  };

  return (
    <div
      data-testid="settings-dialog"
      className="fixed inset-0 bg-black/60 flex items-center justify-center z-50"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-dialog-title"
        className="bg-bg2 rounded-xl border border-white/10 p-6 w-[640px] max-w-[90vw] max-h-[90vh] overflow-y-auto"
      >
        <h2 id="settings-dialog-title" className="text-xl font-medium text-fg mb-4">Settings</h2>

        <div className="mb-4">
          <label className="block text-sm text-fg-dim mb-1">
            Library root (for copy-mode imports)
          </label>
          <div className="flex gap-2">
            <input
              type="text"
              value={libraryRoot ?? ""}
              readOnly
              placeholder="Default: system Pictures folder"
              className="flex-1 px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm"
            />
            <button
              onClick={handleBrowseLibraryRoot}
              title="Pick a library root directory"
              className="px-3 py-2 rounded-lg bg-bg3 text-fg hover:bg-white/10 transition-colors duration-fast text-sm"
            >
              Browse
            </button>
            {libraryRoot !== null && (
              <button
                onClick={handleResetLibraryRoot}
                title="Reset to system default"
                className="px-3 py-2 rounded-lg bg-bg3 text-fg-dim hover:text-fg hover:bg-white/10 transition-colors duration-fast text-sm"
              >
                Reset
              </button>
            )}
          </div>
          <p className="text-xs text-fg-dim mt-1">
            Copy-mode imports create shoots under <code>{"{root}"}/DSLR/YYYY/YYYY-MM_slug/RAW/</code>. In-place imports ignore this.
          </p>
          {libraryRootError && (
            <p className="text-xs text-red-400 mt-1">{libraryRootError}</p>
          )}
        </div>

        <div className="mb-4">
          <label className="block text-sm text-fg-dim mb-1">
            Immich ingest folder (for Publish Direct)
          </label>
          <div className="flex gap-2">
            <input
              type="text"
              value={immichPath ?? ""}
              readOnly
              placeholder="Not configured — Publish Direct disabled"
              className="flex-1 px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm"
            />
            <button
              onClick={handleBrowseImmichPath}
              title="Pick the Immich ingest directory"
              className="px-3 py-2 rounded-lg bg-bg3 text-fg hover:bg-white/10 transition-colors duration-fast text-sm"
            >
              Browse
            </button>
            {immichPath !== null && (
              <button
                onClick={handleResetImmichPath}
                title="Clear (disables Publish Direct)"
                className="px-3 py-2 rounded-lg bg-bg3 text-fg-dim hover:text-fg hover:bg-white/10 transition-colors duration-fast text-sm"
              >
                Reset
              </button>
            )}
          </div>
          <p className="text-xs text-fg-dim mt-1">
            Photos flagged with <kbd className="px-1 bg-bg3 rounded">D</kbd> (publish direct) have their cached JPEG copied here. Re-running skips files that already exist.
          </p>
        </div>

        <FolderLayoutEditor value={folderTemplate} onChange={setFolderTemplate} />

        <div className="mb-4">
          <label className="block text-sm text-fg-dim mb-1">
            Grouping similarity (pHash hamming distance)
          </label>
          <input
            type="number"
            min={0}
            max={64}
            value={threshold}
            onChange={(e) => setThreshold(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm"
          />
          <p className="text-xs text-fg-dim mt-1">
            Higher = looser grouping: more frames cluster together for the
            Select tournament. Default 16.
          </p>
        </div>

        <div className="mb-4">
          <label className="block text-sm text-fg-dim mb-1">
            Group time window (seconds)
          </label>
          <input
            type="number"
            min={0}
            max={3600}
            value={timeWindow}
            onChange={(e) => setTimeWindow(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm"
          />
          <p className="text-xs text-fg-dim mt-1">
            Two photos only cluster if their capture times are within this
            gap AND their pHashes are similar. Blocks cross-moment pHash
            false-positives. Default 60. Set to 0 to disable and use
            pHash-only similarity.
          </p>
        </div>

        <div className="mb-4">
          <label className="flex items-center gap-2 text-sm text-fg cursor-pointer">
            <input
              type="checkbox"
              checked={selectPick}
              onChange={(e) => setSelectPick(e.target.checked)}
              className="w-4 h-4"
            />
            Select view requires pick (hide unreviewed)
          </label>
          <p className="text-xs text-fg-dim mt-1 ml-6">
            When on, photos must pass triage before reaching Select. Off = current behavior (shows picks + unreviewed).
          </p>
        </div>

        <div className="mb-4">
          <label className="block text-sm text-fg-dim mb-1">
            Route minimum star rating (0 = any)
          </label>
          <input
            type="number"
            min={0}
            max={5}
            value={routeStar}
            onChange={(e) => setRouteStar(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm"
          />
          <p className="text-xs text-fg-dim mt-1">
            Route view only shows picks rated ≥ N. Default 3. Set to 0 to disable.
          </p>
        </div>

        {!valid && (
          <div className="text-red-400 text-sm mb-3">
            {folderTemplateErrors.length > 0 ? (
              <ul className="list-disc list-inside space-y-0.5">
                {folderTemplateErrors.map((e) => (
                  <li key={e}>{e}</li>
                ))}
              </ul>
            ) : (
              "Invalid values: thresholds within 0–64 (related ≥ near-duplicate), route star 0–5."
            )}
          </div>
        )}

        {currentShoot && (
          <div className="mb-4 p-3 rounded-lg bg-bg border border-white/5">
            <div className="flex items-center justify-between">
              <span className="text-sm text-fg-dim">
                Re-cluster current shoot with these thresholds
              </span>
              <button
                onClick={handleRecluster}
                disabled={reclustering || !valid}
                className="px-3 py-1.5 rounded bg-bg3 hover:bg-white/10 text-fg text-xs transition-colors duration-fast disabled:opacity-50"
              >
                <BusyLabel busy={reclustering} busyText="Re-clustering…">Re-cluster</BusyLabel>
              </button>
            </div>
            {reclusterMsg && (
              <StatusLine msg={reclusterMsg} />
            )}
          </div>
        )}

        <div className="mb-4 pt-4 border-t border-white/5">
          <h3 className="text-sm font-semibold text-fg mb-1">
            On-device AI
          </h3>
          <p className="text-xs text-fg-dim mb-3">
            Local ONNX models. Computes face detection, eye open/closed,
            smile, and sharpness percentile per photo. Feeds the AI-pick
            badge in the grid and the score bars in the faces rail.
          </p>

          <div className="mb-3 flex items-center justify-between text-sm">
            <span className="text-fg-dim">Inference backend</span>
            <span className={providerLabel(aiProvider).color}>
              {providerLabel(aiProvider).text}
            </span>
          </div>

          <label className="flex items-center gap-2 text-sm text-fg cursor-pointer mb-3">
            <input
              type="checkbox"
              checked={enableAi}
              onChange={(e) => setEnableAi(e.target.checked)}
              className="w-4 h-4"
            />
            Enable on-device AI on import
          </label>
          <p className="text-xs text-fg-dim -mt-2 ml-6 mb-3">
            When on, each import kicks off face + eye + sharpness analysis in the background.
          </p>

          <label className="block text-sm text-fg-dim mb-1">
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
            <div className="mt-4 p-3 rounded-lg bg-bg border border-white/5">
              <div className="flex items-center justify-between">
                <span className="text-sm text-fg-dim">
                  Re-analyze this shoot with on-device AI
                </span>
                <button
                  onClick={handleReanalyze}
                  disabled={reanalyzing}
                  className="px-3 py-1.5 rounded bg-bg3 hover:bg-white/10 text-fg text-xs transition-colors duration-fast disabled:opacity-50"
                >
                  <BusyLabel busy={reanalyzing} busyText="Queuing…">Re-analyze</BusyLabel>
                </button>
              </div>
              {reanalyzeMsg && (
                <StatusLine msg={reanalyzeMsg} />
              )}
            </div>
          )}
        </div>

        <div className="mb-4 pt-4 border-t border-white/5">
          <h3 className="text-sm font-semibold text-fg mb-1">
            Curator (cloud)
          </h3>
          <p className="text-xs text-fg-dim mb-3">
            Compositional and aesthetic judgment via a vision LLM (Anthropic,
            Gemini, or a local OpenAI-compatible endpoint). Computes a
            per-photo keep/toss recommendation, cluster rank, and a written
            reason. Accept with{" "}
            <kbd className="px-1 bg-bg3 rounded">.</kbd> in Triage.
            Cloud keys are stored in the OS keychain — never written to the database.
          </p>

          <label className="block text-sm text-fg-dim mb-1">
            Provider
          </label>
          <div className="flex gap-1 mb-3 p-1 rounded-lg bg-bg border border-white/10">
            {(["anthropic", "gemini", "local"] as const).map((p) => (
              <button
                key={p}
                onClick={() => setCuratorProvider(p)}
                className={`flex-1 px-3 py-1.5 rounded text-sm transition-colors duration-base ${
                  curatorProvider === p
                    ? "bg-accent-blue text-white"
                    : "text-fg-dim hover:bg-white/5"
                }`}
              >
                {providerName(p)}
              </button>
            ))}
          </div>

          {curatorProvider === "anthropic" && (
            <>
              <label className="block text-sm text-fg-dim mb-1">
                Anthropic API key
              </label>
              <div className="flex gap-2 mb-1">
                {keyStatusAnthropic?.configured ? (
                  <input
                    type="text"
                    value={`••••••••${keyStatusAnthropic.suffix}`}
                    readOnly
                    className="flex-1 px-3 py-2 rounded-lg bg-bg text-fg-dim border border-white/10 text-sm font-mono"
                  />
                ) : (
                  <input
                    type="password"
                    value={newKey}
                    onChange={(e) => setNewKey(e.target.value)}
                    placeholder="sk-ant-..."
                    className="flex-1 px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm font-mono"
                    autoComplete="off"
                    spellCheck={false}
                  />
                )}
                {keyStatusAnthropic?.configured ? (
                  <>
                    <button
                      onClick={handleTestConnection}
                      disabled={keyBusy}
                      className="px-3 py-2 rounded-lg bg-bg3 text-fg hover:bg-white/10 transition-colors duration-fast text-sm disabled:opacity-50"
                    >
                      <BusyLabel busy={keyBusy} busyText="Testing…">Test</BusyLabel>
                    </button>
                    <button
                      onClick={handleClearKey}
                      disabled={keyBusy}
                      className="px-3 py-2 rounded-lg bg-bg3 text-fg-dim hover:text-fg hover:bg-white/10 transition-colors duration-fast text-sm disabled:opacity-50"
                    >
                      Replace
                    </button>
                  </>
                ) : (
                  <button
                    onClick={handleSaveKey}
                    disabled={keyBusy || !newKey.trim()}
                    className="px-3 py-2 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-white text-sm transition-colors duration-fast disabled:opacity-50"
                  >
                    Save
                  </button>
                )}
              </div>

              <label className="block text-sm text-fg-dim mb-1 mt-3">
                Model
              </label>
              <select
                value={modelAnthropic}
                onChange={(e) => setModelAnthropic(e.target.value)}
                className="w-full px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm mb-1"
              >
                <option value="claude-sonnet-4-6">claude-sonnet-4-6 (recommended)</option>
                <option value="claude-opus-4-7">claude-opus-4-7 (more thorough, ~5× cost)</option>
                <option value="claude-haiku-4-5">claude-haiku-4-5 (cheaper, less nuance)</option>
              </select>
            </>
          )}

          {curatorProvider === "gemini" && (
            <>
              <label className="block text-sm text-fg-dim mb-1">
                Gemini API key
              </label>
              <div className="flex gap-2 mb-1">
                {keyStatusGemini?.configured ? (
                  <input
                    type="text"
                    value={`••••••••${keyStatusGemini.suffix}`}
                    readOnly
                    className="flex-1 px-3 py-2 rounded-lg bg-bg text-fg-dim border border-white/10 text-sm font-mono"
                  />
                ) : (
                  <input
                    type="password"
                    value={newKey}
                    onChange={(e) => setNewKey(e.target.value)}
                    placeholder="AIza..."
                    className="flex-1 px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm font-mono"
                    autoComplete="off"
                    spellCheck={false}
                  />
                )}
                {keyStatusGemini?.configured ? (
                  <>
                    <button
                      onClick={handleTestConnection}
                      disabled={keyBusy}
                      className="px-3 py-2 rounded-lg bg-bg3 text-fg hover:bg-white/10 transition-colors duration-fast text-sm disabled:opacity-50"
                    >
                      <BusyLabel busy={keyBusy} busyText="Testing…">Test</BusyLabel>
                    </button>
                    <button
                      onClick={handleClearKey}
                      disabled={keyBusy}
                      className="px-3 py-2 rounded-lg bg-bg3 text-fg-dim hover:text-fg hover:bg-white/10 transition-colors duration-fast text-sm disabled:opacity-50"
                    >
                      Replace
                    </button>
                  </>
                ) : (
                  <button
                    onClick={handleSaveKey}
                    disabled={keyBusy || !newKey.trim()}
                    className="px-3 py-2 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-white text-sm transition-colors duration-fast disabled:opacity-50"
                  >
                    Save
                  </button>
                )}
              </div>

              <label className="block text-sm text-fg-dim mb-1 mt-3">
                Model
              </label>
              <select
                value={modelGemini}
                onChange={(e) => setModelGemini(e.target.value)}
                className="w-full px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm mb-1"
              >
                <option value="gemini-2.5-flash">gemini-2.5-flash (recommended)</option>
                <option value="gemini-2.5-pro">gemini-2.5-pro (more thorough, ~5× cost)</option>
                <option value="gemini-2.0-flash">gemini-2.0-flash (cheapest)</option>
              </select>
            </>
          )}

          {curatorProvider === "local" && (
            <>
              <label className="block text-sm text-fg-dim mb-1">
                Base URL
              </label>
              <div className="flex gap-2 mb-3">
                <input
                  type="text"
                  value={localBaseUrl}
                  onChange={(e) => setLocalBaseUrl(e.target.value)}
                  placeholder="http://localhost:11434/v1"
                  spellCheck={false}
                  className="flex-1 px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm font-mono"
                />
                <button
                  onClick={handleTestConnection}
                  disabled={keyBusy || !modelLocal.trim() || !localBaseUrl.trim()}
                  className="px-3 py-2 rounded-lg bg-bg3 text-fg hover:bg-white/10 transition-colors duration-fast text-sm disabled:opacity-50"
                >
                  <BusyLabel busy={keyBusy} busyText="Testing…">Test</BusyLabel>
                </button>
              </div>
              <p className="text-xs text-fg-dim -mt-2 mb-3">
                Works with Ollama, LM Studio, vLLM, llama.cpp server, or anything that
                speaks the OpenAI Chat Completions API.
              </p>

              <label className="block text-sm text-fg-dim mb-1">
                Model
              </label>
              <input
                type="text"
                value={modelLocal}
                onChange={(e) => setModelLocal(e.target.value)}
                placeholder="qwen2-vl:7b · llava:13b · llama3.2-vision:11b"
                spellCheck={false}
                className="w-full px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm font-mono mb-1"
              />
            </>
          )}

          <StatusLine msg={keyMsg} />

          <label className="flex items-center gap-2 text-sm text-fg cursor-pointer mt-3 mb-1">
            <input
              type="checkbox"
              checked={curatorRunOnImport}
              onChange={(e) => setCuratorRunOnImport(e.target.checked)}
              className="w-4 h-4"
              disabled={!activeKeyStatus?.configured}
            />
            Run Curator on import (default for new shoots)
          </label>
          <p className="text-xs text-fg-dim -mt-1 ml-6 mb-3">
            Per-shoot toggle in the import dialog can override this default.
            {curatorProvider === "local"
              ? " Disabled until a model name is set."
              : " Disabled until an API key is configured."}
          </p>

          <label className="flex items-center gap-2 text-sm text-fg cursor-pointer mt-1 mb-1">
            <input
              type="checkbox"
              checked={triageOnImport}
              onChange={(e) => setTriageOnImport(e.target.checked)}
              className="w-4 h-4"
              disabled={!activeKeyStatus?.configured}
            />
            AI triage on import
          </label>
          <p className="text-xs text-fg-dim -mt-1 ml-6 mb-3">
            A fast first pass that auto-rejects only clearly-unusable frames
            (severe blur, closed eyes, blown exposure). Rejects are reviewable
            with the Triage "AI rejects" filter and undoable with Z. Spends a
            small amount of LLM budget per import.
          </p>

          <label className="block text-sm text-fg-dim mb-1">
            {curatorProvider === "local" ? "Max cost per shoot ($, ignored for local)" : "Max cost per shoot ($)"}
          </label>
          <input
            type="number"
            min={0}
            step={0.5}
            value={curatorMaxCostDollars}
            onChange={(e) => setCuratorMaxCostDollars(e.target.value)}
            disabled={curatorProvider === "local"}
            className="w-full px-3 py-2 rounded-lg bg-bg text-fg border border-white/10 text-sm disabled:opacity-50"
          />
          <p className="text-xs text-fg-dim mt-1">
            {curatorProvider === "local"
              ? "Local inference is free; cap is inert. Token counts are tracked instead."
              : "Worker stops dispatching new calls once this is exceeded; in-flight calls finish. Default $5.00."}
          </p>

          {currentShoot && (
            <div className="mt-4 p-3 rounded-lg bg-bg border border-white/5">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-sm text-fg truncate">
                    Run Curator on{" "}
                    <span className="font-mono">{currentShoot.slug}</span>
                  </div>
                  <div className="text-xs text-fg-dim mt-0.5">
                    {currentShoot.photoCount} photos
                    {curatorProvider === "anthropic" && curatorEstimate !== null &&
                      ` · ~${formatCostCents(curatorEstimate)} estimated`}
                    {curatorProvider === "gemini" && " · usage billed by Google"}
                    {curatorProvider === "local" && " · free (local)"}
                  </div>
                </div>
                <div className="flex gap-2 shrink-0">
                  <button
                    onClick={handleRunCurator}
                    disabled={curatorRunning || !activeKeyStatus?.configured || (curatorProvider === "local" && !modelLocal.trim())}
                    className="px-3 py-1.5 rounded bg-bg3 hover:bg-white/10 text-fg text-xs transition-colors duration-fast disabled:opacity-50"
                  >
                    {curatorRunning ? "Starting…" : "Run"}
                  </button>
                  <button
                    onClick={handleRerunCurator}
                    disabled={curatorRunning || !activeKeyStatus?.configured || (curatorProvider === "local" && !modelLocal.trim())}
                    className="px-3 py-1.5 rounded bg-bg3 hover:bg-white/10 text-fg-dim hover:text-fg text-xs transition-colors duration-fast disabled:opacity-50"
                    title="Discard existing judgments and re-run"
                  >
                    Re-run
                  </button>
                </div>
              </div>
              {curatorMsg && (
                <StatusLine msg={curatorMsg} />
              )}
              {curatorProvider !== "local" && !activeKeyStatus?.configured && (
                <p className="text-xs text-fg-dim mt-2">
                  Save an API key above to enable.
                </p>
              )}
              {curatorProvider === "local" && !modelLocal.trim() && (
                <p className="text-xs text-fg-dim mt-2">
                  Set a model name above to enable.
                </p>
              )}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2">
          <button
            onClick={closeDialog}
            className="px-4 py-2 rounded-lg text-fg-dim hover:text-fg transition-colors duration-fast"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!valid}
            className="px-4 py-2 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-white font-medium transition-colors duration-fast disabled:opacity-50"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
