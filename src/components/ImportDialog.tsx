import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSettingsStore } from "../stores/settingsStore";
import {
  estimateCuratorCostCents,
  formatCostCents,
  getCuratorApiKeyStatus,
  startCuratorForShoot,
} from "../lib/curatorApi";
import type { DriveInfo } from "../types";
import { useDriveDetection } from "../hooks/useDriveDetection";
import { DriveSourceBar } from "./import/DriveSourceBar";
import { DateBrowser } from "./import/DateBrowser";
import { FolderSubsetGrid } from "./import/FolderSubsetGrid";

interface ImportProgress {
  shootId: number;
  phase: string;
  current: number;
  total: number;
  currentFilename: string;
}

interface ImportComplete {
  shootId: number;
  photoCount: number;
  dedupSkipped: number;
}

interface ImportDialogProps {
  onClose: () => void;
  onComplete: (shootId: number) => void;
  initialDrive?: DriveInfo | null;
}

type Source =
  | { kind: "drive"; drive: DriveInfo }
  | { kind: "folder"; path: string }
  | null;

type ImportMode = "copy" | "in_place";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function slugifyFolderName(name: string): string {
  return name.replace(/[^a-zA-Z0-9_-]/g, "-");
}

const PIPELINE_STAGES: { label: string; phases: string[] }[] = [
  { label: "copy RAW", phases: ["processing"] },
  { label: "extract preview", phases: ["processing"] },
  { label: "read EXIF", phases: ["processing"] },
  { label: "p-hash group", phases: ["clustering"] },
  { label: "sharpness", phases: ["finalizing"] },
  { label: "face · eye · smile", phases: ["finalizing"] },
];

function PipelineRow({
  label,
  current,
  total,
  running,
}: {
  label: string;
  current: number;
  total: number;
  running: boolean;
}) {
  const pct = total > 0 ? Math.min(100, (current / total) * 100) : 0;
  return (
    <div>
      <div
        className="flex justify-between font-mono text-[10px] mb-[3px]"
        style={{ color: running ? "var(--color-fg)" : "var(--color-fg-dim)" }}
      >
        <span>{label}</span>
        <span>
          {current}/{total}
        </span>
      </div>
      <div
        className="h-[2px] rounded-[1px] overflow-hidden"
        style={{ background: "var(--color-bg3)" }}
      >
        <div
          className="h-full"
          style={{
            width: `${pct}%`,
            background: running ? "var(--color-accent)" : "var(--color-fg-mute)",
          }}
        />
      </div>
    </div>
  );
}

export function ImportDialog({ onClose, onComplete, initialDrive }: ImportDialogProps) {
  const { drives } = useDriveDetection();
  const [source, setSource] = useState<Source>(null);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [slug, setSlug] = useState("");
  const [slugDirty, setSlugDirty] = useState(false);
  const [importMode, setImportMode] = useState<ImportMode>("copy");
  const [importing, setImporting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const initialDriveAppliedRef = useRef(false);

  // Honor an external request (e.g. from the drive-detected toast) to
  // pre-select a specific drive once on mount. After that, the user is
  // free to switch sources without us re-overriding.
  useEffect(() => {
    if (initialDriveAppliedRef.current) return;
    if (!initialDrive) return;
    initialDriveAppliedRef.current = true;
    setSource({ kind: "drive", drive: initialDrive });
  }, [initialDrive]);

  // Curator auto-run on import. The provider is whichever one the user
  // has currently selected in Settings; the prerequisite differs:
  // cloud providers need an API key, local needs a model name.
  const settings = useSettingsStore((s) => s.settings);
  const [curatorEnabled, setCuratorEnabled] = useState(settings.curatorDefaultRunOnImport);
  const [curatorReady, setCuratorReady] = useState(false);
  const curatorProvider = settings.curatorProvider;
  const curatorProviderLabel =
    curatorProvider === "anthropic"
      ? "Anthropic"
      : curatorProvider === "gemini"
        ? "Gemini"
        : "Local";
  // We store the *intent* in a ref so the import-complete listener
  // (which runs after this dialog re-renders into its final progress
  // state) can read the right value without a stale closure.
  const curatorEnabledRef = useRef(curatorEnabled);
  useEffect(() => {
    curatorEnabledRef.current = curatorEnabled;
  }, [curatorEnabled]);
  useEffect(() => {
    if (curatorProvider === "local") {
      setCuratorReady(settings.curatorModelLocal.trim().length > 0);
      return;
    }
    getCuratorApiKeyStatus(curatorProvider)
      .then((s) => setCuratorReady(s.configured))
      .catch(() => setCuratorReady(false));
  }, [curatorProvider, settings.curatorModelLocal]);
  // When prerequisites aren't met, force the checkbox off regardless of
  // the saved default — we can't run the curator without them.
  useEffect(() => {
    if (!curatorReady && curatorEnabled) setCuratorEnabled(false);
  }, [curatorReady, curatorEnabled]);

  // Forced copy mode for cards; folder source preserves the toggle.
  useEffect(() => {
    if (source?.kind === "drive") setImportMode("copy");
  }, [source]);

  const handleSelectDrive = useCallback((drive: DriveInfo) => {
    setSource({ kind: "drive", drive });
    setSelectedPaths([]);
    setTotalBytes(0);
    setSlugDirty(false);
  }, []);

  const handleSelectFolder = useCallback((path: string) => {
    setSource({ kind: "folder", path });
    setSelectedPaths([]);
    setTotalBytes(0);
    if (!slugDirty) {
      const name = path.split(/[/\\]/).pop() || "";
      setSlug(slugifyFolderName(name));
    }
  }, [slugDirty]);

  // DateBrowser provides a suggested slug derived from drive label +
  // newest-day; honor it unless the user has typed their own.
  const handleDateSelectionChange = useCallback(
    (paths: string[], bytes: number, suggestedSlug: string) => {
      setSelectedPaths(paths);
      setTotalBytes(bytes);
      if (!slugDirty) setSlug(suggestedSlug);
    },
    [slugDirty],
  );

  const handleFolderSelectionChange = useCallback(
    (paths: string[], bytes: number) => {
      setSelectedPaths(paths);
      setTotalBytes(bytes);
    },
    [],
  );

  // Subscribe to import-progress / -complete / -error while the import
  // backend thread is running. Detached from source-selection so the
  // listeners don't churn each time the user clicks around.
  useEffect(() => {
    if (!importing) return;

    const unlistenProgress = listen<ImportProgress>("import-progress", (event) => {
      setProgress(event.payload);
    });

    const unlistenComplete = listen<ImportComplete>("import-complete", (event) => {
      setImporting(false);
      setCancelling(false);
      // Auto-start the curator on the new shoot if the user opted in
      // for this import. Read the ref so we don't capture stale state.
      if (curatorEnabledRef.current) {
        startCuratorForShoot(event.payload.shootId).catch((e) => {
          // Don't block import success on curator start failure —
          // surface the error but let the import flow proceed.
          console.error("Failed to start curator:", e);
        });
      }
      onComplete(event.payload.shootId);
    });

    const unlistenError = listen<string>("import-error", (event) => {
      setImporting(false);
      setCancelling(false);
      setError(event.payload);
    });

    return () => {
      unlistenProgress.then((fn) => fn()).catch(() => {});
      unlistenComplete.then((fn) => fn()).catch(() => {});
      unlistenError.then((fn) => fn()).catch(() => {});
    };
  }, [importing, onComplete]);

  const handleStart = useCallback(async () => {
    if (!source) return;
    if (!slug.trim()) {
      setError("Give this import a name.");
      return;
    }
    if (selectedPaths.length === 0) {
      setError("Select at least one photo to import.");
      return;
    }
    setError(null);
    setImporting(true);
    try {
      const sourcePath =
        source.kind === "drive" ? source.drive.mountPoint : source.path;
      await invoke("start_import", {
        sourcePath,
        slug: slug.trim(),
        importMode,
        selectedPaths,
      });
    } catch (e) {
      setError(String(e));
      setImporting(false);
    }
  }, [source, slug, importMode, selectedPaths]);

  // Photo count that will actually be imported, used for the curator
  // cost estimate. The new SD-card / folder browsers feed `selectedPaths`
  // directly, so the count is just its length.
  const importingPhotoCount = selectedPaths.length;

  const [curatorCostCents, setCuratorCostCents] = useState<number | null>(null);
  useEffect(() => {
    if (!curatorEnabled || importingPhotoCount === 0) {
      setCuratorCostCents(null);
      return;
    }
    let cancelled = false;
    estimateCuratorCostCents(importingPhotoCount)
      .then((c) => {
        if (!cancelled) setCuratorCostCents(c);
      })
      .catch(() => {
        if (!cancelled) setCuratorCostCents(null);
      });
    return () => {
      cancelled = true;
    };
  }, [curatorEnabled, importingPhotoCount]);

  const dialogWidthClass = source ? "w-[960px]" : "w-[600px]";

  const phaseRunning = (label: (typeof PIPELINE_STAGES)[number]["label"]): boolean => {
    if (!progress) return false;
    const stage = PIPELINE_STAGES.find((s) => s.label === label);
    if (!stage) return false;
    return stage.phases.includes(progress.phase);
  };

  const importLabel = useMemo(() => {
    if (selectedPaths.length === 0) return "Import";
    return `Import ${selectedPaths.length} ${selectedPaths.length === 1 ? "photo" : "photos"}`;
  }, [selectedPaths.length]);

  return (
    <div className="fixed inset-0 bg-black/55 flex items-center justify-center z-50">
      <div
        className={`rounded-md max-w-[95vw] max-h-[90vh] overflow-hidden flex flex-col p-5 ${dialogWidthClass}`}
        style={{
          background: "var(--color-bg2)",
          border: "1px solid var(--color-border)",
          boxShadow: "0 20px 80px rgba(0,0,0,0.5)",
        }}
      >
        <div
          className="text-[14px] font-semibold mb-4"
          style={{ color: "var(--color-fg)" }}
        >
          Import
        </div>

        {!importing ? (
          <div className="flex-1 overflow-y-auto">
            <div className="mb-4">
              <DriveSourceBar
                drives={drives}
                selectedMountPoint={
                  source?.kind === "drive" ? source.drive.mountPoint : null
                }
                selectedFolderPath={
                  source?.kind === "folder" ? source.path : null
                }
                onSelectDrive={handleSelectDrive}
                onSelectFolder={handleSelectFolder}
              />
            </div>

            {source?.kind === "drive" && (
              <DateBrowser
                drive={source.drive}
                onSelectionChange={handleDateSelectionChange}
              />
            )}

            {source?.kind === "folder" && (
              <FolderSubsetGrid
                folderPath={source.path}
                onSelectionChange={handleFolderSelectionChange}
              />
            )}

            {!source && (
              <div
                className="text-[12px] py-6 px-3"
                style={{ color: "var(--color-fg-dim)" }}
              >
                Pick a source above to begin. PhotoSift detects SD cards and
                external drives automatically — they'll appear here within a few
                seconds of plugging in.
              </div>
            )}

            {source?.kind === "folder" && (
              <div className="mb-4 mt-4">
                <div
                  className="text-[11px] mb-[6px]"
                  style={{ color: "var(--color-fg-dim)" }}
                >
                  Import mode
                </div>
                <label className="flex items-start gap-2 cursor-pointer py-1">
                  <input
                    type="radio"
                    name="import-mode"
                    value="copy"
                    checked={importMode === "copy"}
                    onChange={() => setImportMode("copy")}
                    className="mt-[2px]"
                  />
                  <div>
                    <div className="text-[12px]" style={{ color: "var(--color-fg)" }}>
                      Copy to library
                    </div>
                    <div className="text-[10px]" style={{ color: "var(--color-fg-dim)" }}>
                      Files are copied into a canonical folder under the library
                      root.
                    </div>
                  </div>
                </label>
                <label className="flex items-start gap-2 cursor-pointer py-1">
                  <input
                    type="radio"
                    name="import-mode"
                    value="in_place"
                    checked={importMode === "in_place"}
                    onChange={() => setImportMode("in_place")}
                    className="mt-[2px]"
                  />
                  <div>
                    <div className="text-[12px]" style={{ color: "var(--color-fg)" }}>
                      Import in-place
                    </div>
                    <div className="text-[10px]" style={{ color: "var(--color-fg-dim)" }}>
                      Register files where they are. XMP sidecars land next to
                      the originals on export.
                    </div>
                  </div>
                </label>
              </div>
            )}

            {error && (
              <p
                className="text-[12px] mt-3 mb-2"
                style={{ color: "var(--color-danger)" }}
              >
                {error}
              </p>
            )}

            {source && (
              <label
                className="flex items-start gap-2 mb-4 cursor-pointer"
                title={
                  curatorReady
                    ? undefined
                    : curatorProvider === "local"
                      ? "Set a model name for the local provider in Settings to enable AI suggestions."
                      : `Configure your ${curatorProviderLabel} API key in Settings to enable AI suggestions.`
                }
              >
                <input
                  type="checkbox"
                  checked={curatorEnabled}
                  onChange={(e) => setCuratorEnabled(e.target.checked)}
                  disabled={!curatorReady}
                  className="mt-[2px] cursor-pointer disabled:cursor-not-allowed"
                />
                <div>
                  <div
                    className="text-[12px]"
                    style={{
                      color: curatorReady
                        ? "var(--color-fg)"
                        : "var(--color-fg-mute)",
                    }}
                  >
                    Run AI suggestions on import
                    <span
                      className="ml-2 font-mono text-[10px]"
                      style={{ color: "var(--color-fg-dim)" }}
                    >
                      (via {curatorProviderLabel})
                    </span>
                    {curatorEnabled &&
                      curatorProvider === "anthropic" &&
                      curatorCostCents !== null && (
                        <span
                          className="ml-2 font-mono text-[10px]"
                          style={{ color: "var(--color-fg-dim)" }}
                        >
                          (~{formatCostCents(curatorCostCents)} estimated for{" "}
                          {importingPhotoCount}{" "}
                          {importingPhotoCount === 1 ? "photo" : "photos"})
                        </span>
                      )}
                  </div>
                  <div
                    className="text-[10px]"
                    style={{ color: "var(--color-fg-dim)" }}
                  >
                    {curatorReady
                      ? "AI characterizes the shoot then evaluates each cluster for composition + aesthetics. You can accept its suggestions with `.` in Triage."
                      : curatorProvider === "local"
                        ? "Set a model name for the local provider in Settings to enable."
                        : `Configure a ${curatorProviderLabel} API key in Settings to enable.`}
                  </div>
                </div>
              </label>
            )}

            <div
              className="flex items-center gap-3 pt-3 mt-3 border-t"
              style={{ borderColor: "var(--color-border)" }}
            >
              <div
                className="text-[11px] flex-shrink-0"
                style={{ color: "var(--color-fg-dim)" }}
              >
                Name
              </div>
              <input
                type="text"
                value={slug}
                onChange={(e) => {
                  setSlug(e.target.value);
                  setSlugDirty(true);
                }}
                placeholder="e.g. 2026-03-05_nikon-d750"
                className="flex-1 px-[10px] py-[6px] rounded-md text-[12px] font-mono"
                style={{
                  background: "var(--color-bg3)",
                  border: "1px solid var(--color-border)",
                  color: "var(--color-fg)",
                }}
              />
              <div
                className="text-[11px] font-mono whitespace-nowrap"
                style={{ color: "var(--color-fg-dim)" }}
              >
                {selectedPaths.length > 0 && (
                  <>
                    {selectedPaths.length} · {formatBytes(totalBytes)}
                  </>
                )}
              </div>
              <button
                type="button"
                onClick={onClose}
                className="px-[14px] py-[6px] rounded-md text-[12px] cursor-pointer"
                style={{
                  background: "transparent",
                  color: "var(--color-fg-dim)",
                  border: "1px solid var(--color-border)",
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleStart}
                disabled={!source || !slug.trim() || selectedPaths.length === 0}
                className="px-[14px] py-[6px] rounded-md text-[12px] font-medium cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                style={{
                  background: "var(--color-accent-blue)",
                  color: "#fff",
                  border: "none",
                }}
              >
                {importLabel}
              </button>
            </div>
          </div>
        ) : (
          <div>
            <div
              className="text-[12px] mb-1"
              style={{
                color: cancelling
                  ? "var(--color-danger)"
                  : "var(--color-fg)",
              }}
            >
              {cancelling
                ? "Cancelling… waiting for in-flight files to finish."
                : "Processing files…"}
            </div>
            <div
              className="h-1 mt-[10px] rounded-sm overflow-hidden"
              style={{ background: "var(--color-bg3)" }}
            >
              <div
                className="h-full transition-all"
                style={{
                  width:
                    progress && progress.total > 0
                      ? `${Math.round((progress.current / progress.total) * 100)}%`
                      : "0%",
                  background: "var(--color-accent-blue)",
                }}
              />
            </div>
            <div
              className="flex justify-between font-mono text-[10px] mt-2"
              style={{ color: "var(--color-fg-dim)" }}
            >
              <span className="truncate">
                {progress
                  ? `${progress.current} / ${progress.total}${
                      progress.currentFilename ? ` · ${progress.currentFilename}` : ""
                    }`
                  : "starting…"}
              </span>
              <button
                type="button"
                disabled={cancelling}
                onClick={async () => {
                  setCancelling(true);
                  try {
                    await invoke("cancel_import");
                  } catch {
                    // already-finished imports surface as errors here; ignore.
                  }
                }}
                className="text-[11px] cursor-pointer bg-transparent border-0 disabled:opacity-50 disabled:cursor-default"
                style={{ color: "var(--color-danger)" }}
              >
                {cancelling ? "Cancelling…" : "Cancel import"}
              </button>
            </div>
            <div
              className="mt-4 pt-[14px] grid grid-cols-2 gap-[6px]"
              style={{ borderTop: "1px solid var(--color-border)" }}
            >
              {PIPELINE_STAGES.map((stage) => (
                <PipelineRow
                  key={stage.label}
                  label={stage.label}
                  current={progress?.current ?? 0}
                  total={progress?.total ?? 0}
                  running={phaseRunning(stage.label)}
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
