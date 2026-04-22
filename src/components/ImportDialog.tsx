import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

interface ScanEntry {
  path: string;
  filename: string;
  capturedAt: string | null;
  fileSizeBytes: number;
  thumbDataUrl: string | null;
}

interface ScanProgress {
  index: number;
  total: number;
  entry: ScanEntry;
}

function compareEntries(a: ScanEntry, b: ScanEntry): number {
  if (a.capturedAt && b.capturedAt) {
    if (a.capturedAt !== b.capturedAt) return a.capturedAt < b.capturedAt ? -1 : 1;
  } else if (a.capturedAt) {
    return -1;
  } else if (b.capturedAt) {
    return 1;
  }
  return a.filename.localeCompare(b.filename);
}

interface ImportDialogProps {
  onClose: () => void;
  onComplete: (shootId: number) => void;
}

type ImportMode = "copy" | "in_place";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
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

export function ImportDialog({ onClose, onComplete }: ImportDialogProps) {
  const [sourcePath, setSourcePath] = useState("");
  const [slug, setSlug] = useState("");
  const [importMode, setImportMode] = useState<ImportMode>("copy");
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [scanning, setScanning] = useState(false);
  const [scanEntries, setScanEntries] = useState<ScanEntry[] | null>(null);
  const [scanProgress, setScanProgress] = useState<{ done: number; total: number } | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [selectSubset, setSelectSubset] = useState(false);
  const lastClickedIdx = useRef<number | null>(null);

  const runScan = useCallback(async (source: string, withThumbnails: boolean) => {
    setScanning(true);
    setScanEntries([]);
    setScanProgress(null);
    setError(null);
    setSelected(new Set());

    const unlisten = await listen<ScanProgress>("scan-progress", (event) => {
      const { entry } = event.payload;
      setScanEntries((prev) => {
        const next = prev ? [...prev, entry] : [entry];
        next.sort(compareEntries);
        return next;
      });
      setSelected((prev) => {
        const next = new Set(prev);
        next.add(entry.path);
        return next;
      });
      setScanProgress({ done: event.payload.index + 1, total: event.payload.total });
    });

    try {
      await invoke<number>("scan_folder", { source, withThumbnails });
    } catch (e) {
      setError(`Scan failed: ${e}`);
      setScanEntries([]);
    } finally {
      unlisten();
      setScanning(false);
    }
  }, []);

  const handlePickFolder = useCallback(async () => {
    const picked = await open({ directory: true });
    if (typeof picked !== "string") return;
    setSourcePath(picked);
    if (!slug) {
      const name = picked.split(/[/\\]/).pop() || "";
      setSlug(name.replace(/[^a-zA-Z0-9_-]/g, "-"));
    }
    runScan(picked, selectSubset);
  }, [slug, runScan, selectSubset]);

  const toggleSelectSubset = useCallback(() => {
    const next = !selectSubset;
    setSelectSubset(next);
    if (sourcePath && !scanning) {
      runScan(sourcePath, next);
    }
  }, [selectSubset, sourcePath, scanning, runScan]);

  useEffect(() => {
    if (!importing) return;

    const unlistenProgress = listen<ImportProgress>("import-progress", (event) => {
      setProgress(event.payload);
    });

    const unlistenComplete = listen<ImportComplete>("import-complete", (event) => {
      setImporting(false);
      onComplete(event.payload.shootId);
    });

    const unlistenError = listen<string>("import-error", (event) => {
      setImporting(false);
      setError(event.payload);
    });

    return () => {
      unlistenProgress.then((fn) => fn()).catch(() => {});
      unlistenComplete.then((fn) => fn()).catch(() => {});
      unlistenError.then((fn) => fn()).catch(() => {});
    };
  }, [importing, onComplete]);

  const handleStart = useCallback(async () => {
    if (!sourcePath || !slug.trim()) return;
    const selectedPaths = Array.from(selected);
    if (selectedPaths.length === 0) {
      setError("Select at least one photo to import.");
      return;
    }

    setError(null);
    setImporting(true);

    try {
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
  }, [sourcePath, slug, importMode, selected]);

  const toggleOne = useCallback(
    (idx: number, ev: React.MouseEvent | null) => {
      if (!scanEntries) return;
      const entry = scanEntries[idx];
      if (!entry) return;
      setSelected((prev) => {
        const next = new Set(prev);
        if (ev?.shiftKey && lastClickedIdx.current != null) {
          const from = Math.min(lastClickedIdx.current, idx);
          const to = Math.max(lastClickedIdx.current, idx);
          const willBeOn = !next.has(entry.path);
          for (let i = from; i <= to; i++) {
            const p = scanEntries[i].path;
            if (willBeOn) next.add(p);
            else next.delete(p);
          }
        } else {
          if (next.has(entry.path)) next.delete(entry.path);
          else next.add(entry.path);
        }
        return next;
      });
      lastClickedIdx.current = idx;
    },
    [scanEntries],
  );

  const selectAll = useCallback(() => {
    if (!scanEntries) return;
    setSelected(new Set(scanEntries.map((e) => e.path)));
  }, [scanEntries]);

  const selectNone = useCallback(() => {
    setSelected(new Set());
  }, []);

  const totalBytes = useMemo(() => {
    if (!scanEntries) return 0;
    let sum = 0;
    for (const e of scanEntries) if (selected.has(e.path)) sum += e.fileSizeBytes;
    return sum;
  }, [scanEntries, selected]);

  const hasScan = scanEntries !== null && scanEntries.length > 0;
  const dialogWidthClass =
    (hasScan || scanning) && selectSubset ? "w-[960px]" : "w-[420px]";

  const phaseRunning = (label: (typeof PIPELINE_STAGES)[number]["label"]): boolean => {
    if (!progress) return false;
    const stage = PIPELINE_STAGES.find((s) => s.label === label);
    if (!stage) return false;
    return stage.phases.includes(progress.phase);
  };

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
          Import Photos
        </div>

        {!importing ? (
          <div className="flex-1 overflow-y-auto">
            <div className="mb-4">
              <div
                className="text-[11px] mb-[6px]"
                style={{ color: "var(--color-fg-dim)" }}
              >
                Import mode
              </div>
              <label className="flex items-start gap-2 cursor-pointer py-2">
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
                  <div
                    className="text-[10px]"
                    style={{ color: "var(--color-fg-dim)" }}
                  >
                    Files are copied into a canonical folder under the library root.
                  </div>
                </div>
              </label>
              <label className="flex items-start gap-2 cursor-pointer py-2">
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
                  <div
                    className="text-[10px]"
                    style={{ color: "var(--color-fg-dim)" }}
                  >
                    Register files where they are. XMP sidecars land next to the originals on export.
                  </div>
                </div>
              </label>
            </div>

            <div className="mb-4">
              <div
                className="text-[11px] mb-1"
                style={{ color: "var(--color-fg-dim)" }}
              >
                Source folder
              </div>
              <div className="flex gap-[6px]">
                <input
                  type="text"
                  value={sourcePath}
                  readOnly
                  placeholder="Select folder…"
                  className="flex-1 px-[10px] py-[6px] rounded-md text-[11px] font-mono"
                  style={{
                    background: "var(--color-bg3)",
                    border: "1px solid var(--color-border)",
                    color: "var(--color-fg)",
                  }}
                />
                <button
                  type="button"
                  onClick={handlePickFolder}
                  className="px-[12px] py-[5px] rounded-md text-[12px] cursor-pointer"
                  style={{
                    background: "transparent",
                    border: "1px solid var(--color-border)",
                    color: "var(--color-fg)",
                  }}
                >
                  Browse
                </button>
              </div>
            </div>

            <div className="mb-4">
              <div
                className="text-[11px] mb-1"
                style={{ color: "var(--color-fg-dim)" }}
              >
                Description
              </div>
              <input
                type="text"
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
                placeholder="e.g. Greece-Trip"
                className="w-full px-[10px] py-[6px] rounded-md text-[12px]"
                style={{
                  background: "var(--color-bg3)",
                  border: "1px solid var(--color-border)",
                  color: "var(--color-fg)",
                }}
              />
            </div>

            {sourcePath && (
              <label className="flex items-start gap-2 mb-4 cursor-pointer">
                <input
                  type="checkbox"
                  checked={selectSubset}
                  onChange={toggleSelectSubset}
                  disabled={scanning}
                  className="mt-[2px] cursor-pointer"
                />
                <div>
                  <div
                    className="text-[12px]"
                    style={{ color: "var(--color-fg)" }}
                  >
                    Select subset
                  </div>
                  <div
                    className="text-[10px]"
                    style={{ color: "var(--color-fg-dim)" }}
                  >
                    Load thumbnails so you can deselect unwanted photos. Off by default — embedded previews take a while on RAW folders.
                  </div>
                </div>
              </label>
            )}

            {scanning && (
              <div
                className="mb-4 p-[10px] rounded-md text-[11px] font-mono"
                style={{
                  background: "var(--color-bg3)",
                  border: "1px solid var(--color-border)",
                  color: "var(--color-fg-dim)",
                }}
              >
                {scanProgress
                  ? selectSubset
                    ? `Loading thumbnails… ${scanProgress.done} of ${scanProgress.total}`
                    : `Scanning folder… ${scanProgress.done} of ${scanProgress.total}`
                  : "Scanning folder…"}
              </div>
            )}

            {hasScan && !selectSubset && (
              <div
                className="mb-4 p-[10px] rounded-md"
                style={{ background: "var(--color-bg3)" }}
              >
                <div
                  className="text-[12px]"
                  style={{ color: "var(--color-fg)" }}
                >
                  {scanEntries!.length}{" "}
                  {scanEntries!.length === 1 ? "photo" : "photos"} ready to import
                </div>
                <div
                  className="text-[11px] mt-[2px] font-mono"
                  style={{ color: "var(--color-fg-dim)" }}
                >
                  {formatBytes(totalBytes)} · everything under the source folder will be imported
                </div>
              </div>
            )}

            {hasScan && selectSubset && (
              <div className="mb-4">
                <div className="flex items-center justify-between mb-2">
                  <div
                    className="text-[12px]"
                    style={{ color: "var(--color-fg-dim)" }}
                  >
                    {selected.size} of {scanEntries!.length} selected
                    <span className="ml-2 opacity-60">
                      ({formatBytes(totalBytes)})
                    </span>
                  </div>
                  <div className="flex gap-1">
                    <button
                      type="button"
                      onClick={selectAll}
                      className="px-[8px] py-[3px] rounded-xs text-[11px] cursor-pointer"
                      style={{
                        background: "var(--color-bg3)",
                        color: "var(--color-fg)",
                        border: "1px solid var(--color-border)",
                      }}
                    >
                      All
                    </button>
                    <button
                      type="button"
                      onClick={selectNone}
                      className="px-[8px] py-[3px] rounded-xs text-[11px] cursor-pointer"
                      style={{
                        background: "var(--color-bg3)",
                        color: "var(--color-fg)",
                        border: "1px solid var(--color-border)",
                      }}
                    >
                      None
                    </button>
                  </div>
                </div>
                <p
                  className="text-[10px] mb-2"
                  style={{ color: "var(--color-fg-mute)" }}
                >
                  Click to toggle. Shift-click to toggle a range.
                </p>
                <div className="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-2 max-h-[50vh] overflow-y-auto pr-1">
                  {scanEntries!.map((entry, idx) => {
                    const isSelected = selected.has(entry.path);
                    return (
                      <div
                        key={entry.path}
                        onClick={(e) => toggleOne(idx, e)}
                        className={`relative aspect-[3/2] rounded-xs overflow-hidden cursor-pointer border-2 transition-all ${
                          isSelected ? "" : "opacity-40 hover:opacity-70"
                        }`}
                        style={{
                          borderColor: isSelected
                            ? "var(--color-accent-blue)"
                            : "transparent",
                        }}
                        title={`${entry.filename} · ${formatBytes(entry.fileSizeBytes)}${entry.capturedAt ? ` · ${entry.capturedAt}` : ""}`}
                      >
                        {entry.thumbDataUrl ? (
                          <img
                            src={entry.thumbDataUrl}
                            alt={entry.filename}
                            loading="lazy"
                            draggable={false}
                            className="w-full h-full object-cover bg-black/50"
                          />
                        ) : (
                          <div
                            className="w-full h-full flex items-center justify-center text-[10px] px-1 text-center"
                            style={{
                              background: "var(--color-bg3)",
                              color: "var(--color-fg-dim)",
                            }}
                          >
                            {entry.filename}
                          </div>
                        )}
                        {isSelected && (
                          <div
                            className="absolute top-1 right-1 w-4 h-4 rounded-full text-white text-[10px] flex items-center justify-center font-bold"
                            style={{ background: "var(--color-accent-blue)" }}
                          >
                            {"✓"}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {scanEntries !== null && scanEntries.length === 0 && !scanning && (
              <p
                className="text-[12px] mb-4"
                style={{ color: "var(--color-fg-dim)" }}
              >
                No supported image files found in that folder.
              </p>
            )}

            {error && (
              <p
                className="text-[12px] mb-4"
                style={{ color: "var(--color-danger)" }}
              >
                {error}
              </p>
            )}

            <div
              className="flex justify-end gap-2 pt-3 mt-2 border-t"
              style={{ borderColor: "var(--color-border)" }}
            >
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
                disabled={
                  !sourcePath ||
                  !slug.trim() ||
                  scanning ||
                  (hasScan && selectSubset && selected.size === 0)
                }
                className="px-[14px] py-[6px] rounded-md text-[12px] font-medium cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                style={{
                  background: "var(--color-accent-blue)",
                  color: "#fff",
                  border: "none",
                }}
              >
                {hasScan
                  ? selectSubset
                    ? `Import ${selected.size} ${selected.size === 1 ? "photo" : "photos"}`
                    : `Import all ${scanEntries!.length}`
                  : "Start import"}
              </button>
            </div>
          </div>
        ) : (
          <div>
            <div
              className="text-[12px] mb-1"
              style={{ color: "var(--color-fg)" }}
            >
              Processing files…
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
                onClick={async () => {
                  try {
                    await invoke("cancel_import");
                  } catch {
                    /* ignore */
                  }
                }}
                className="text-[11px] cursor-pointer bg-transparent border-0"
                style={{ color: "var(--color-danger)" }}
              >
                Cancel import
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
