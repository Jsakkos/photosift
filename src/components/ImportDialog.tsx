import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSettingsStore } from "../stores/settingsStore";
import { useProjectStore } from "../stores/projectStore";
import { useModalA11y } from "../hooks/useModalA11y";
import {
  clearCuratorForShoot,
  estimateCuratorCostCents,
  formatCostCents,
  getCuratorApiKeyStatus,
  getCuratorJudgmentsForShoot,
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
  /// When set, the dialog adds photos to this existing shoot (#12) instead
  /// of creating a new one: no slug prompt, no import-mode toggle, and a
  /// "re-run Curator" prompt after ingest if the shoot has judgments.
  targetShoot?: { id: number; slug: string; date: string } | null;
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
        className="flex justify-between font-mono text-2xs mb-[3px]"
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

export function ImportDialog({
  onClose,
  onComplete,
  initialDrive,
  targetShoot,
}: ImportDialogProps) {
  const addMode = targetShoot != null;
  const setToast = useProjectStore((s) => s.setToast);
  const { drives } = useDriveDetection();
  const [source, setSource] = useState<Source>(null);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [slug, setSlug] = useState(targetShoot?.slug ?? "");
  const [slugDirty, setSlugDirty] = useState(false);
  const [importMode, setImportMode] = useState<ImportMode>("copy");
  // #4: a single user-controlled toggle for "skip duplicates", applied
  // identically to drive (DateBrowser) and folder (FolderSubsetGrid)
  // sources. Default ON — matches the pre-#4 implicit behaviour. When OFF,
  // the backend bypasses the cross-shoot hash check; the per-shoot
  // UNIQUE constraint still blocks same-shoot duplicates.
  const [skipDuplicates, setSkipDuplicates] = useState(true);
  const [importing, setImporting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const initialDriveAppliedRef = useRef(false);

  // Escape / focus-trap / focus-restore for the dialog. Disabled while an
  // import is running — closing mid-run has its own Cancel control.
  const dialogRef = useRef<HTMLDivElement>(null);
  useModalA11y(dialogRef, onClose, !importing);

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
      const shootId = event.payload.shootId;
      if (addMode) {
        // Adding a batch can change cluster membership and stale any cached
        // Curator judgments — offer a one-click re-run if there are any.
        getCuratorJudgmentsForShoot(shootId)
          .then((judgments) => {
            if (judgments.length === 0) return;
            setToast(
              `Added ${event.payload.photoCount} photo${event.payload.photoCount === 1 ? "" : "s"} — Curator judgments may be stale.`,
              "info",
              {
                label: "Re-run Curator",
                onClick: () => {
                  clearCuratorForShoot(shootId)
                    .then(() => startCuratorForShoot(shootId))
                    .catch((e) => console.error("Failed to re-run curator:", e));
                },
              },
            );
          })
          .catch(() => {});
      } else if (curatorEnabledRef.current) {
        // Auto-start the curator on the new shoot if the user opted in
        // for this import. Read the ref so we don't capture stale state.
        startCuratorForShoot(shootId).catch((e) => {
          // Don't block import success on curator start failure —
          // surface the error but let the import flow proceed.
          console.error("Failed to start curator:", e);
        });
      }
      onComplete(shootId);
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
  }, [importing, onComplete, addMode, setToast]);

  const handleStart = useCallback(async () => {
    if (!source) return;
    if (!addMode && !slug.trim()) {
      setError("Give this import a name.");
      return;
    }
    if (selectedPaths.length === 0) {
      setError("Select at least one photo to import.");
      return;
    }
    const sourcePath =
      source.kind === "drive" ? source.drive.mountPoint : source.path;

    // Adding to an existing shoot: warn if the new batch's month doesn't
    // match the shoot's, since that usually means the wrong folder.
    if (addMode && targetShoot) {
      try {
        const derived = await invoke<string>("derive_import_year_month", {
          sourcePath,
        });
        const shootMonth = targetShoot.date.slice(0, 7); // "YYYY-MM-01" -> "YYYY-MM"
        if (derived !== shootMonth) {
          const ok = window.confirm(
            `These photos look like ${derived}, but "${targetShoot.slug}" is a ${shootMonth} shoot.\n\nAdd them to this shoot anyway?`,
          );
          if (!ok) return;
        }
      } catch {
        // Non-fatal: if we can't derive the date, fall through and import.
      }
    }

    setError(null);
    setImporting(true);
    try {
      await invoke("start_import", {
        sourcePath,
        slug: slug.trim(),
        importMode,
        selectedPaths,
        existingShootId: targetShoot?.id ?? null,
        skipDuplicates,
      });
    } catch (e) {
      setError(String(e));
      setImporting(false);
    }
  }, [source, slug, importMode, selectedPaths, skipDuplicates, addMode, targetShoot]);

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
    const verb = addMode ? "Add" : "Import";
    if (selectedPaths.length === 0) return verb;
    return `${verb} ${selectedPaths.length} ${selectedPaths.length === 1 ? "photo" : "photos"}`;
  }, [selectedPaths.length, addMode]);

  return (
    <div
      data-testid="import-dialog"
      className="fixed inset-0 bg-black/55 flex items-center justify-center z-50"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-dialog-title"
        className={`rounded-md max-w-[95vw] max-h-[90vh] overflow-hidden flex flex-col p-5 ${dialogWidthClass}`}
        style={{
          background: "var(--color-bg2)",
          border: "1px solid var(--color-border)",
          boxShadow: "0 20px 80px rgba(0,0,0,0.5)",
        }}
      >
        <div
          id="import-dialog-title"
          className="text-sm font-semibold mb-4"
          style={{ color: "var(--color-fg)" }}
        >
          {addMode ? `Add photos to ${targetShoot!.slug}` : "Import"}
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
                className="text-xs py-6 px-3"
                style={{ color: "var(--color-fg-dim)" }}
              >
                Pick a source above to begin. PhotoSift detects SD cards and
                external drives automatically — they'll appear here within a few
                seconds of plugging in.
              </div>
            )}

            {source?.kind === "folder" && !addMode && (
              <div className="mb-4 mt-4">
                <div
                  className="text-[11px] mb-1.5"
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
                    className="mt-0.5"
                  />
                  <div>
                    <div className="text-xs" style={{ color: "var(--color-fg)" }}>
                      Copy to library
                    </div>
                    <div className="text-2xs" style={{ color: "var(--color-fg-dim)" }}>
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
                    className="mt-0.5"
                  />
                  <div>
                    <div className="text-xs" style={{ color: "var(--color-fg)" }}>
                      Import in-place
                    </div>
                    <div className="text-2xs" style={{ color: "var(--color-fg-dim)" }}>
                      Register files where they are. XMP sidecars land next to
                      the originals on export.
                    </div>
                  </div>
                </label>
              </div>
            )}

            {error && (
              <p
                className="text-xs mt-3 mb-2"
                style={{ color: "var(--color-danger)" }}
              >
                {error}
              </p>
            )}

            {source && (
              <label className="flex items-start gap-2 mb-3 cursor-pointer">
                <input
                  type="checkbox"
                  checked={skipDuplicates}
                  onChange={(e) => setSkipDuplicates(e.target.checked)}
                  className="mt-0.5 cursor-pointer"
                />
                <div>
                  <div className="text-xs" style={{ color: "var(--color-fg)" }}>
                    Skip duplicates
                  </div>
                  <div
                    className="text-2xs"
                    style={{ color: "var(--color-fg-dim)" }}
                  >
                    {skipDuplicates
                      ? "Files already imported in any shoot are skipped."
                      : "Duplicates can land in this shoot too — the same RAW will live in both shoots."}
                  </div>
                </div>
              </label>
            )}

            {source && !addMode && (
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
                  className="mt-0.5 cursor-pointer disabled:cursor-not-allowed"
                />
                <div>
                  <div
                    className="text-xs"
                    style={{
                      color: curatorReady
                        ? "var(--color-fg)"
                        : "var(--color-fg-mute)",
                    }}
                  >
                    Run Curator on import
                    <span
                      className="ml-2 font-mono text-2xs"
                      style={{ color: "var(--color-fg-dim)" }}
                    >
                      (via {curatorProviderLabel})
                    </span>
                    {curatorEnabled &&
                      curatorProvider === "anthropic" &&
                      curatorCostCents !== null && (
                        <span
                          className="ml-2 font-mono text-2xs"
                          style={{ color: "var(--color-fg-dim)" }}
                        >
                          (~{formatCostCents(curatorCostCents)} estimated for{" "}
                          {importingPhotoCount}{" "}
                          {importingPhotoCount === 1 ? "photo" : "photos"})
                        </span>
                      )}
                  </div>
                  <div
                    className="text-2xs"
                    style={{ color: "var(--color-fg-dim)" }}
                  >
                    {curatorReady
                      ? "The Curator characterizes the shoot then evaluates each cluster for composition + aesthetics. You can accept its suggestions with `.` in Triage."
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
                {addMode ? "Adding to" : "Name"}
              </div>
              <input
                type="text"
                value={slug}
                onChange={(e) => {
                  setSlug(e.target.value);
                  setSlugDirty(true);
                }}
                readOnly={addMode}
                placeholder="e.g. 2026-03-05_nikon-d750"
                className="flex-1 px-2.5 py-1.5 rounded-md text-xs font-mono read-only:opacity-70"
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
                className="px-3.5 py-1.5 rounded-md text-xs cursor-pointer"
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
                className="px-3.5 py-1.5 rounded-md text-xs font-medium cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
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
              className="text-xs mb-1"
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
              className="h-1 mt-2.5 rounded-sm overflow-hidden"
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
              className="flex justify-between font-mono text-2xs mt-2"
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
              className="mt-4 pt-3.5 grid grid-cols-2 gap-1.5"
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
