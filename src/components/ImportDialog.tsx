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

export function ImportDialog({ onClose, onComplete }: ImportDialogProps) {
  const [sourcePath, setSourcePath] = useState("");
  const [slug, setSlug] = useState("");
  const [importMode, setImportMode] = useState<ImportMode>("copy");
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Pre-import scan state
  const [scanning, setScanning] = useState(false);
  const [scanEntries, setScanEntries] = useState<ScanEntry[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  // shift-click range selection uses the last-clicked index as anchor
  const lastClickedIdx = useRef<number | null>(null);

  const runScan = useCallback(async (source: string) => {
    setScanning(true);
    setScanEntries(null);
    setError(null);
    setSelected(new Set());
    try {
      const entries = await invoke<ScanEntry[]>("scan_folder", { source });
      setScanEntries(entries);
      // Default: every scan result selected. User can deselect from there.
      setSelected(new Set(entries.map((e) => e.path)));
    } catch (e) {
      setError(`Scan failed: ${e}`);
      setScanEntries([]);
    } finally {
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
    runScan(picked);
  }, [slug, runScan]);

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
        // Shift-click: toggle the whole range between the last-click anchor
        // and the current index to match what the entry at idx is about to become.
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

  const phaseLabel = progress?.phase === "processing"
    ? "Processing files..."
    : progress?.phase === "clustering"
    ? "Clustering groups..."
    : progress?.phase === "finalizing"
    ? "Saving to database..."
    : "Scanning...";

  const hasScan = scanEntries !== null && scanEntries.length > 0;
  const dialogWidthClass = hasScan || scanning ? "w-[960px]" : "w-[480px]";

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className={`bg-[var(--bg-secondary)] rounded-xl border border-white/10 p-6 max-w-[95vw] max-h-[90vh] overflow-hidden flex flex-col ${dialogWidthClass}`}>
        <h2 className="text-xl font-medium text-[var(--text-primary)] mb-4">
          Import Photos
        </h2>

        {!importing ? (
          <div className="flex-1 overflow-y-auto">
            <div className="mb-4">
              <label className="block text-sm text-[var(--text-secondary)] mb-2">
                Import Mode
              </label>
              <div className="flex flex-col gap-2">
                <label className="flex items-start gap-2 cursor-pointer text-sm">
                  <input
                    type="radio"
                    name="import-mode"
                    value="copy"
                    checked={importMode === "copy"}
                    onChange={() => setImportMode("copy")}
                    className="mt-0.5"
                  />
                  <span>
                    <span className="text-[var(--text-primary)]">Copy to library</span>
                    <span className="block text-xs text-[var(--text-secondary)]">
                      Files are copied into a canonical folder under the library root.
                    </span>
                  </span>
                </label>
                <label className="flex items-start gap-2 cursor-pointer text-sm">
                  <input
                    type="radio"
                    name="import-mode"
                    value="in_place"
                    checked={importMode === "in_place"}
                    onChange={() => setImportMode("in_place")}
                    className="mt-0.5"
                  />
                  <span>
                    <span className="text-[var(--text-primary)]">Import in-place</span>
                    <span className="block text-xs text-[var(--text-secondary)]">
                      Register files where they are. XMP sidecars land next to the originals on export.
                    </span>
                  </span>
                </label>
              </div>
            </div>

            <div className="mb-4">
              <label className="block text-sm text-[var(--text-secondary)] mb-1">
                Source Folder
              </label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={sourcePath}
                  readOnly
                  placeholder="Select folder..."
                  className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
                />
                <button
                  onClick={handlePickFolder}
                  className="px-3 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-primary)] hover:bg-white/10 transition-colors text-sm"
                >
                  Browse
                </button>
              </div>
            </div>

            <div className="mb-4">
              <label className="block text-sm text-[var(--text-secondary)] mb-1">
                Description
              </label>
              <input
                type="text"
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
                placeholder="e.g. Greece-Trip"
                className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
              />
            </div>

            {scanning && (
              <div className="mb-4 p-3 rounded-lg bg-[var(--bg-primary)] border border-white/5 text-sm text-[var(--text-secondary)]">
                Scanning folder...
              </div>
            )}

            {hasScan && (
              <div className="mb-4">
                <div className="flex items-center justify-between mb-2">
                  <div className="text-sm text-[var(--text-secondary)]">
                    {selected.size} of {scanEntries!.length} selected
                    <span className="ml-2 opacity-60">
                      ({formatBytes(totalBytes)})
                    </span>
                  </div>
                  <div className="flex gap-1">
                    <button
                      type="button"
                      onClick={selectAll}
                      className="px-2 py-1 rounded text-xs bg-[var(--bg-tertiary)] hover:bg-white/10 text-[var(--text-primary)]"
                    >
                      All
                    </button>
                    <button
                      type="button"
                      onClick={selectNone}
                      className="px-2 py-1 rounded text-xs bg-[var(--bg-tertiary)] hover:bg-white/10 text-[var(--text-primary)]"
                    >
                      None
                    </button>
                  </div>
                </div>
                <p className="text-[11px] text-[var(--text-secondary)]/60 mb-2">
                  Click to toggle. Shift-click to toggle a range.
                </p>
                <div className="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-2 max-h-[50vh] overflow-y-auto pr-1">
                  {scanEntries!.map((entry, idx) => {
                    const isSelected = selected.has(entry.path);
                    return (
                      <div
                        key={entry.path}
                        onClick={(e) => toggleOne(idx, e)}
                        className={`relative aspect-[3/2] rounded overflow-hidden cursor-pointer border-2 transition-all ${
                          isSelected
                            ? "border-[var(--accent)]"
                            : "border-transparent opacity-40 hover:opacity-70"
                        }`}
                        title={`${entry.filename} \u00b7 ${formatBytes(entry.fileSizeBytes)}${entry.capturedAt ? ` \u00b7 ${entry.capturedAt}` : ""}`}
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
                          <div className="w-full h-full bg-[var(--bg-primary)] flex items-center justify-center text-[10px] text-[var(--text-secondary)]/70 px-1 text-center">
                            {entry.filename}
                          </div>
                        )}
                        {isSelected && (
                          <div className="absolute top-1 right-1 w-4 h-4 rounded-full bg-[var(--accent)] text-white text-[10px] flex items-center justify-center font-bold">
                            {"\u2713"}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {scanEntries !== null && scanEntries.length === 0 && !scanning && (
              <p className="text-[var(--text-secondary)] text-sm mb-4">
                No supported image files found in that folder.
              </p>
            )}

            {error && (
              <p className="text-red-400 text-sm mb-4">{error}</p>
            )}

            <div className="flex justify-end gap-2 pt-2 border-t border-white/5 mt-2">
              <button
                onClick={onClose}
                className="px-4 py-2 rounded-lg text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleStart}
                disabled={
                  !sourcePath ||
                  !slug.trim() ||
                  scanning ||
                  (hasScan && selected.size === 0)
                }
                className="px-4 py-2 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors disabled:opacity-50"
              >
                {hasScan
                  ? `Import ${selected.size} ${selected.size === 1 ? "photo" : "photos"}`
                  : "Start Import"}
              </button>
            </div>
          </div>
        ) : (
          <div>
            <p className="text-[var(--text-primary)] mb-2">{phaseLabel}</p>
            {progress && progress.total > 0 && (
              <>
                <div className="w-full h-2 bg-[var(--bg-primary)] rounded-full overflow-hidden mb-2">
                  <div
                    className="h-full bg-[var(--accent)] transition-all"
                    style={{
                      width: `${Math.round((progress.current / progress.total) * 100)}%`,
                    }}
                  />
                </div>
                <p className="text-sm text-[var(--text-secondary)]">
                  {progress.current} / {progress.total}
                  {progress.currentFilename && (
                    <span className="ml-2 opacity-70">{progress.currentFilename}</span>
                  )}
                </p>
              </>
            )}
            <div className="flex justify-end mt-4">
              <button
                onClick={async () => {
                  try { await invoke("cancel_import"); } catch {}
                }}
                className="px-4 py-2 rounded-lg text-red-400 hover:text-red-300 hover:bg-red-400/10 transition-colors text-sm"
              >
                Cancel Import
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
