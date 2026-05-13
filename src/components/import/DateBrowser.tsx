import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  // Paths the backend reported `thumbDataUrl: null` for. Without this set,
  // every grouped-state change re-issues an extract for failed paths and
  // we get stuck in a tight retry loop hammering the SD card. Cleared
  // for a day's paths when the user manually re-expands the day, so the
  // close-and-reopen workaround still works as a manual retry.
  const [failedThumbs, setFailedThumbs] = useState<Set<string>>(new Set());

  // Tracks whether we've already auto-expanded the newest day for this
  // scan. The auto-expand fires as soon as the newest day key is known
  // (well before scanComplete) so thumbnails for today's shoot start
  // streaming within seconds of mounting the dialog. After it fires once
  // the user owns the UI â€” never auto-expand again.
  const autoExpandedRef = useRef(false);
  // Once the user manually toggles selection (clicks a tile, All, or None),
  // we stop maintaining the auto pre-selection. Without this, late-arriving
  // entries on the newest day would re-add anything the user just removed.
  const userTouchedSelectionRef = useRef(false);

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
    setFailedThumbs(new Set());
    autoExpandedRef.current = false;
    userTouchedSelectionRef.current = false;

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
      if (!thumbDataUrl) {
        setFailedThumbs((prev) => {
          if (prev.has(path)) return prev;
          const next = new Set(prev);
          next.add(path);
          return next;
        });
        return;
      }
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

  // Auto-expand the newest day as soon as we know what it is (typically
  // within the first scan-progress event, since the walker now feeds files
  // newest-first). Fires once per scan; the user owns the UI thereafter.
  useEffect(() => {
    if (autoExpandedRef.current) return;
    if (!newestDayKey) return;
    const day = grouped.find((g) => g.key === newestDayKey);
    if (!day || day.entries.length === 0) return;
    autoExpandedRef.current = true;
    setExpandedDays((prev) => {
      if (prev.has(newestDayKey)) return prev;
      const next = new Set(prev);
      next.add(newestDayKey);
      return next;
    });
  }, [newestDayKey, grouped]);

  // Pre-select all non-deduped entries in the newest day. This re-runs as
  // more entries stream in (so a current shoot with 50 frames ends up fully
  // selected even though the first effect ran with only 1â€“2 entries known)
  // and stops the moment the user touches selection.
  useEffect(() => {
    if (userTouchedSelectionRef.current) return;
    if (!newestDayKey) return;
    const day = grouped.find((g) => g.key === newestDayKey);
    if (!day) return;
    const desired = new Set<string>();
    for (const e of day.entries) {
      if (!e.alreadyImported) desired.add(e.path);
    }
    setSelected((prev) => {
      if (prev.size === desired.size) {
        let same = true;
        for (const p of desired) if (!prev.has(p)) { same = false; break; }
        if (same) return prev;
      }
      return desired;
    });
  }, [newestDayKey, grouped]);

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
      // Skip when a fetch for this day is already in-flight. The
      // auto-fetch effect re-runs as new entries stream in, and without
      // this gate we'd spawn overlapping rayon pools all decoding from
      // the same slow SD card.
      if (thumbsLoadingDays.has(key)) return;
      // Exclude both successfully-loaded and known-failed paths.
      // Failed paths only get retried on manual day re-expand.
      const missing = dayEntries
        .map((e) => e.path)
        .filter((p) => !loadedThumbs.has(p) && !failedThumbs.has(p));
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
    [loadedThumbs, thumbsLoadingDays, failedThumbs],
  );

  const toggleExpand = useCallback(
    (key: string, visibleEntries: ScanDateEntry[]) => {
      setExpandedDays((prev) => {
        const next = new Set(prev);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
          // Manual expand is also the user's "retry" gesture â€” clear
          // failed entries for this day so requestThumbsForDay will
          // re-attempt them.
          setFailedThumbs((prevFailed) => {
            let mutated = false;
            const nextFailed = new Set(prevFailed);
            for (const e of visibleEntries) {
              if (nextFailed.delete(e.path)) mutated = true;
            }
            return mutated ? nextFailed : prevFailed;
          });
          requestThumbsForDay(key, visibleEntries);
        }
        return next;
      });
    },
    [requestThumbsForDay],
  );

  // Once the newest day is auto-expanded, fetch thumbnails for its visible
  // entries â€” and keep doing so as more entries arrive so freshly-discovered
  // frames populate alongside the rest. `requestThumbsForDay` already
  // dedups against `loadedThumbs`, so re-running with a superset is cheap.
  useEffect(() => {
    if (!autoExpandedRef.current || !newestDayKey) return;
    const day = grouped.find((g) => g.key === newestDayKey);
    if (!day) return;
    const visible = dedupOnly
      ? day.entries.filter((e) => !e.alreadyImported)
      : day.entries;
    if (visible.length === 0) return;
    requestThumbsForDay(newestDayKey, visible);
  }, [newestDayKey, grouped, dedupOnly, requestThumbsForDay]);

  const togglePath = useCallback(
    (_clickedPath: string, _ev: React.MouseEvent | null, range: string[]) => {
      userTouchedSelectionRef.current = true;
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
        className="p-3 rounded-md text-xs"
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
              {"  Â·  "}
              <span style={{ color: "var(--color-fg)" }}>{totalNew} new</span>
              {"  Â·  "}
              {totalImported} already
            </>
          )}
          {!scanComplete && scanProgress && (
            <span style={{ color: "var(--color-fg-mute)" }}>
              {"  Â· scanning "}
              {scanProgress.done}/{scanProgress.total}
            </span>
          )}
        </div>
      </div>

      {scanComplete && totalNew === 0 && (
        <div
          className="p-2.5 rounded-md text-xs mb-3"
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
                userTouchedSelectionRef.current = true;
                setSelected((prev) => {
                  const next = new Set(prev);
                  for (const e of visible) next.add(e.path);
                  return next;
                });
              }}
              onSelectNoneInDay={() => {
                userTouchedSelectionRef.current = true;
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
            className="p-2.5 rounded-md text-xs"
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
