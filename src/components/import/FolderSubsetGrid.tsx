import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface FolderScanEntry {
  path: string;
  filename: string;
  capturedAt: string | null;
  fileSizeBytes: number;
  thumbDataUrl: string | null;
}

interface FolderScanProgress {
  index: number;
  total: number;
  entry: FolderScanEntry;
}

interface FolderSubsetGridProps {
  folderPath: string;
  onSelectionChange: (selectedPaths: string[], totalBytes: number) => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function compareEntries(a: FolderScanEntry, b: FolderScanEntry): number {
  if (a.capturedAt && b.capturedAt) {
    if (a.capturedAt !== b.capturedAt) return a.capturedAt < b.capturedAt ? -1 : 1;
  } else if (a.capturedAt) {
    return -1;
  } else if (b.capturedAt) {
    return 1;
  }
  return a.filename.localeCompare(b.filename);
}

export function FolderSubsetGrid({ folderPath, onSelectionChange }: FolderSubsetGridProps) {
  const [scanning, setScanning] = useState(false);
  const [entries, setEntries] = useState<FolderScanEntry[] | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [selectSubset, setSelectSubset] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lastClickedIdx = useRef<number | null>(null);

  const runScan = useCallback(async (source: string, withThumbnails: boolean) => {
    setScanning(true);
    setEntries([]);
    setProgress(null);
    setError(null);
    setSelected(new Set());

    const unlisten = await listen<FolderScanProgress>("scan-progress", (event) => {
      const { entry } = event.payload;
      setEntries((prev) => {
        const next = prev ? [...prev, entry] : [entry];
        next.sort(compareEntries);
        return next;
      });
      setSelected((prev) => {
        const next = new Set(prev);
        next.add(entry.path);
        return next;
      });
      setProgress({ done: event.payload.index + 1, total: event.payload.total });
    });

    try {
      await invoke<number>("scan_folder", { source, withThumbnails });
    } catch (e) {
      setError(`Scan failed: ${e}`);
      setEntries([]);
    } finally {
      unlisten();
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    if (!folderPath) return;
    runScan(folderPath, selectSubset);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [folderPath]);

  const totalBytes = entries
    ? entries.reduce((sum, e) => (selected.has(e.path) ? sum + e.fileSizeBytes : sum), 0)
    : 0;

  useEffect(() => {
    onSelectionChange(Array.from(selected), totalBytes);
  }, [selected, totalBytes, onSelectionChange]);

  const toggleSelectSubset = useCallback(() => {
    const next = !selectSubset;
    setSelectSubset(next);
    if (folderPath && !scanning) runScan(folderPath, next);
  }, [selectSubset, folderPath, scanning, runScan]);

  const toggleOne = useCallback(
    (idx: number, ev: React.MouseEvent | null) => {
      if (!entries) return;
      const entry = entries[idx];
      if (!entry) return;
      setSelected((prev) => {
        const next = new Set(prev);
        if (ev?.shiftKey && lastClickedIdx.current != null) {
          const from = Math.min(lastClickedIdx.current, idx);
          const to = Math.max(lastClickedIdx.current, idx);
          const willBeOn = !next.has(entry.path);
          for (let i = from; i <= to; i++) {
            const p = entries[i].path;
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
    [entries],
  );

  const selectAll = () => entries && setSelected(new Set(entries.map((e) => e.path)));
  const selectNone = () => setSelected(new Set());

  return (
    <div>
      <label className="flex items-start gap-2 mb-3 cursor-pointer">
        <input
          type="checkbox"
          checked={selectSubset}
          onChange={toggleSelectSubset}
          disabled={scanning}
          className="mt-0.5 cursor-pointer"
        />
        <div>
          <div className="text-xs" style={{ color: "var(--color-fg)" }}>
            Select subset
          </div>
          <div className="text-2xs" style={{ color: "var(--color-fg-dim)" }}>
            Load thumbnails so you can deselect unwanted photos. Off by default —
            embedded previews take a while on RAW folders.
          </div>
        </div>
      </label>

      {scanning && (
        <div
          className="mb-3 p-2.5 rounded-md text-[11px] font-mono"
          style={{
            background: "var(--color-bg3)",
            border: "1px solid var(--color-border)",
            color: "var(--color-fg-dim)",
          }}
        >
          {progress
            ? selectSubset
              ? `Loading thumbnails… ${progress.done} of ${progress.total}`
              : `Scanning folder… ${progress.done} of ${progress.total}`
            : "Scanning folder…"}
        </div>
      )}

      {entries !== null && entries.length > 0 && !selectSubset && (
        <div
          className="mb-3 p-2.5 rounded-md"
          style={{ background: "var(--color-bg3)" }}
        >
          <div className="text-xs" style={{ color: "var(--color-fg)" }}>
            {entries.length} {entries.length === 1 ? "photo" : "photos"} ready to import
          </div>
          <div
            className="text-[11px] mt-0.5 font-mono"
            style={{ color: "var(--color-fg-dim)" }}
          >
            {formatBytes(totalBytes)} · everything under the source folder will be imported
          </div>
        </div>
      )}

      {entries !== null && entries.length > 0 && selectSubset && (
        <div className="mb-3">
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs" style={{ color: "var(--color-fg-dim)" }}>
              {selected.size} of {entries.length} selected
              <span className="ml-2 opacity-60">({formatBytes(totalBytes)})</span>
            </div>
            <div className="flex gap-1">
              <button
                type="button"
                onClick={selectAll}
                className="px-2 py-[3px] rounded-xs text-[11px] cursor-pointer"
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
                className="px-2 py-[3px] rounded-xs text-[11px] cursor-pointer"
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
          <p className="text-2xs mb-2" style={{ color: "var(--color-fg-mute)" }}>
            Click to toggle. Shift-click to toggle a range.
          </p>
          <div className="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-2 max-h-[50vh] overflow-y-auto pr-1">
            {entries.map((entry, idx) => {
              const isSelected = selected.has(entry.path);
              return (
                <div
                  key={entry.path}
                  onClick={(e) => toggleOne(idx, e)}
                  className={`relative aspect-[3/2] rounded-xs overflow-hidden cursor-pointer border-2 transition-all duration-base ${
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
                      className="w-full h-full flex items-center justify-center text-2xs px-1 text-center"
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
                      className="absolute top-1 right-1 w-4 h-4 rounded-full text-white text-2xs flex items-center justify-center font-bold"
                      style={{ background: "var(--color-accent-blue)" }}
                    >
                      ✓
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {entries !== null && entries.length === 0 && !scanning && (
        <p className="text-xs mb-3" style={{ color: "var(--color-fg-dim)" }}>
          No supported image files found in that folder.
        </p>
      )}

      {error && (
        <p className="text-xs mb-3" style={{ color: "var(--color-danger)" }}>
          {error}
        </p>
      )}
    </div>
  );
}
