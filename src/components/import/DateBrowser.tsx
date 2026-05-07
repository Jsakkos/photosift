import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  extractThumbnailsForPaths,
  scanFolderForDates,
} from "../../lib/importApi";
import {
  deriveSlug,
  groupByDay,
  newestDayKey as findNewestDayKey,
  relativeDayLabel,
} from "../../lib/dateBrowser";
import type { DriveInfo, ScanDateEntry, ScanThumbReady } from "../../types";
import { DayRow } from "./DayRow";

interface DateBrowserProps {
  drive: DriveInfo;
  onSelectionChange: (
    selectedPaths: string[],
    totalBytes: number,
    suggestedSlug: string,
  ) => void;
}

interface ScanProgressPayload {
  index: number;
  total: number;
  entry: ScanDateEntry;
}

export function DateBrowser({ drive, onSelectionChange }: DateBrowserProps) {
  const [entries, setEntries] = useState<ScanDateEntry[]>([]);
  const [scanComplete, setScanComplete] = useState(false);
  const [scanProgress, setScanProgress] = useState<{ done: number; total: number } | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [expandedDays, setExpandedDays] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [dedupOnly, setDedupOnly] = useState(true);
  const [loadedThumbs, setLoadedThumbs] = useState<Map<string, string>>(new Map());
  const [thumbsLoadingDays, setThumbsLoadingDays] = useState<Set<string>>(new Set());

  // Subscribe to backend events and kick off the scan when drive changes.
  useEffect(() => {
    let cancelled = false;
    setEntries([]);
    setScanComplete(false);
    setScanProgress(null);
    setScanError(null);
    setExpandedDays(new Set());
    setSelected(new Set());
    setLoadedThumbs(new Map());
    setThumbsLoadingDays(new Set());

    const unlistenProgress = listen<ScanProgressPayload>("scan-progress", (event) => {
      if (cancelled) return;
      const { entry, index, total } = event.payload;
      setScanProgress({ done: index + 1, total });
      setEntries((prev) => [...prev, entry]);
    });

    const unlistenComplete = listen<number>("scan-complete", () => {
      if (cancelled) return;
      setScanComplete(true);
    });

    const unlistenThumb = listen<ScanThumbReady>("scan-thumb-ready", (event) => {
      if (cancelled) return;
      const { path, thumbDataUrl } = event.payload;
      if (!thumbDataUrl) return;
      setLoadedThumbs((prev) => {
        const next = new Map(prev);
        next.set(path, thumbDataUrl);
        return next;
      });
    });

    scanFolderForDates(drive.mountPoint, true).catch((e) => {
      if (!cancelled) setScanError(String(e));
    });

    return () => {
      cancelled = true;
      unlistenProgress.then((fn) => fn()).catch(() => {});
      unlistenComplete.then((fn) => fn()).catch(() => {});
      unlistenThumb.then((fn) => fn()).catch(() => {});
    };
  }, [drive.mountPoint]);

  const grouped = useMemo(() => groupByDay(entries), [entries]);
  const newestDayKey = useMemo(() => findNewestDayKey(grouped), [grouped]);

  // On scan-complete, auto-expand the newest day and pre-select all of
  // its non-deduped photos. Only run once per scan.
  useEffect(() => {
    if (!scanComplete) return;
    if (!newestDayKey) return;
    setExpandedDays((prev) => {
      if (prev.has(newestDayKey)) return prev;
      const next = new Set(prev);
      next.add(newestDayKey);
      return next;
    });
    setSelected((prev) => {
      if (prev.size > 0) return prev;
      const next = new Set<string>();
      const day = grouped.find((g) => g.key === newestDayKey);
      if (!day) return prev;
      for (const e of day.entries) {
        if (!e.alreadyImported) next.add(e.path);
      }
      return next;
    });
  }, [scanComplete, newestDayKey, grouped]);

  const totalSeen = entries.length;
  const totalNew = useMemo(
    () => entries.reduce((n, e) => (e.alreadyImported ? n : n + 1), 0),
    [entries],
  );
  const totalImported = totalSeen - totalNew;

  const totalBytesSelected = useMemo(() => {
    let sum = 0;
    for (const e of entries) if (selected.has(e.path)) sum += e.fileSizeBytes;
    return sum;
  }, [entries, selected]);

  const suggestedSlug = useMemo(
    () => deriveSlug(drive.label, drive.driveLetter, newestDayKey),
    [drive.label, drive.driveLetter, newestDayKey],
  );

  // Bubble up selection state to the parent footer.
  useEffect(() => {
    onSelectionChange(Array.from(selected), totalBytesSelected, suggestedSlug);
  }, [selected, totalBytesSelected, suggestedSlug, onSelectionChange]);

  const requestThumbsForDay = useCallback(
    (key: string, dayEntries: ScanDateEntry[]) => {
      const missing = dayEntries
        .map((e) => e.path)
        .filter((p) => !loadedThumbs.has(p));
      if (missing.length === 0) return;
      setThumbsLoadingDays((prev) => {
        const next = new Set(prev);
        next.add(key);
        return next;
      });
      extractThumbnailsForPaths(missing).finally(() => {
        setThumbsLoadingDays((prev) => {
          const next = new Set(prev);
          next.delete(key);
          return next;
        });
      });
    },
    [loadedThumbs],
  );

  const toggleExpand = useCallback(
    (key: string, visibleEntries: ScanDateEntry[]) => {
      setExpandedDays((prev) => {
        const next = new Set(prev);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
          requestThumbsForDay(key, visibleEntries);
        }
        return next;
      });
    },
    [requestThumbsForDay],
  );

  // When scan-complete auto-expands the newest day, fetch its thumbs too.
  useEffect(() => {
    if (!scanComplete || !newestDayKey) return;
    const day = grouped.find((g) => g.key === newestDayKey);
    if (!day) return;
    const visible = dedupOnly
      ? day.entries.filter((e) => !e.alreadyImported)
      : day.entries;
    requestThumbsForDay(newestDayKey, visible);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scanComplete, newestDayKey]);

  const togglePath = useCallback(
    (_clickedPath: string, _ev: React.MouseEvent | null, range: string[]) => {
      setSelected((prev) => {
        const next = new Set(prev);
        const wantOn = !range.every((p) => next.has(p));
        for (const p of range) {
          if (wantOn) next.add(p);
          else next.delete(p);
        }
        return next;
      });
    },
    [],
  );

  const today = new Date();

  if (scanError) {
    return (
      <div
        className="p-3 rounded-md text-[12px]"
        style={{
          background: "var(--color-bg3)",
          color: "var(--color-danger)",
          border: "1px solid var(--color-border)",
        }}
      >
        Scan failed: {scanError}
      </div>
    );
  }

  return (
    <div>
      <div
        className="flex items-center justify-between mb-3 text-[11px] font-mono"
        style={{ color: "var(--color-fg-dim)" }}
      >
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={dedupOnly}
            onChange={(e) => setDedupOnly(e.target.checked)}
            className="cursor-pointer"
          />
          <span style={{ color: "var(--color-fg)" }}>Hide already-imported</span>
        </label>
        <div>
          {totalSeen} photos
          {scanComplete && (
            <>
              {"  ·  "}
              <span style={{ color: "var(--color-fg)" }}>{totalNew} new</span>
              {"  ·  "}
              {totalImported} already
            </>
          )}
          {!scanComplete && scanProgress && (
            <span style={{ color: "var(--color-fg-mute)" }}>
              {"  · scanning "}
              {scanProgress.done}/{scanProgress.total}
            </span>
          )}
        </div>
      </div>

      {scanComplete && totalNew === 0 && (
        <div
          className="p-[10px] rounded-md text-[12px] mb-3"
          style={{
            background: "var(--color-bg3)",
            color: "var(--color-fg-dim)",
            border: "1px solid var(--color-border)",
          }}
        >
          All {totalSeen} photos on this card are already in PhotoSift.{" "}
          <button
            type="button"
            onClick={() => setDedupOnly(false)}
            className="cursor-pointer underline"
            style={{ color: "var(--color-accent-blue)" }}
          >
            Show all
          </button>
        </div>
      )}

      <div className="max-h-[55vh] overflow-y-auto pr-1">
        {grouped.map(({ key, entries: dayEntries }) => {
          const visible = dedupOnly
            ? dayEntries.filter((e) => !e.alreadyImported)
            : dayEntries;
          const hidden = dedupOnly
            ? dayEntries.length - visible.length
            : dayEntries.filter((e) => e.alreadyImported).length;
          if (dedupOnly && visible.length === 0 && hidden === 0) return null;
          const expanded = expandedDays.has(key);
          const dayLabel = relativeDayLabel(key, today);
          const loaded = visible.filter((e) => loadedThumbs.has(e.path)).length;
          return (
            <DayRow
              key={key}
              dayLabel={dayLabel}
              visibleEntries={visible}
              hiddenImportedCount={hidden}
              expanded={expanded}
              onToggleExpand={() => toggleExpand(key, visible)}
              selected={selected}
              onTogglePath={togglePath}
              onSelectAllInDay={() => {
                setSelected((prev) => {
                  const next = new Set(prev);
                  for (const e of visible) next.add(e.path);
                  return next;
                });
              }}
              onSelectNoneInDay={() => {
                setSelected((prev) => {
                  const next = new Set(prev);
                  for (const e of visible) next.delete(e.path);
                  return next;
                });
              }}
              loadedThumbs={loadedThumbs}
              thumbsLoading={thumbsLoadingDays.has(key)}
              thumbsLoaded={loaded}
            />
          );
        })}
        {grouped.length === 0 && scanComplete && (
          <div
            className="p-[10px] rounded-md text-[12px]"
            style={{
              background: "var(--color-bg3)",
              color: "var(--color-fg-dim)",
              border: "1px solid var(--color-border)",
            }}
          >
            No images found on this drive.
          </div>
        )}
      </div>
    </div>
  );
}
