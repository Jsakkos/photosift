import { useRef } from "react";
import type { ScanDateEntry } from "../../types";

interface DayRowProps {
  dayLabel: string;
  visibleEntries: ScanDateEntry[];
  hiddenImportedCount: number;
  expanded: boolean;
  onToggleExpand: () => void;
  selected: Set<string>;
  onTogglePath: (path: string, ev: React.MouseEvent | null, dayPaths: string[]) => void;
  onSelectAllInDay: () => void;
  onSelectNoneInDay: () => void;
  loadedThumbs: Map<string, string>;
  thumbsLoading: boolean;
  thumbsLoaded: number;
}

function formatHM(captured: string | null): string | null {
  if (!captured) return null;
  // EXIF formats: "YYYY:MM:DD HH:MM:SS" or "YYYY-MM-DD HH:MM:SS"
  const parts = captured.split(/[ :T-]/);
  if (parts.length < 5) return null;
  const hh = parts[3];
  const mm = parts[4];
  if (!hh || !mm) return null;
  return `${hh}:${mm}`;
}

function timeRange(entries: ScanDateEntry[]): string | null {
  const stamps: number[] = [];
  for (const e of entries) {
    if (!e.capturedAt) continue;
    const cleaned = e.capturedAt.replace(/^(\d{4}):(\d{2}):(\d{2})/, "$1-$2-$3");
    const t = Date.parse(cleaned.replace(" ", "T"));
    if (!Number.isNaN(t)) stamps.push(t);
  }
  if (stamps.length === 0) return null;
  stamps.sort((a, b) => a - b);
  const lo = formatHM(new Date(stamps[0]).toISOString().replace("T", " "));
  const hi = formatHM(new Date(stamps[stamps.length - 1]).toISOString().replace("T", " "));
  if (!lo || !hi || lo === hi) return lo;
  return `${lo}–${hi}`;
}

function aspectFor(entry: ScanDateEntry): string {
  const portraitOrientations = new Set([5, 6, 7, 8]);
  // Without explicit orientation we default to 3/2 landscape (D750 native).
  // The UI tolerates the 50/50 mix gracefully — strips wrap.
  const isPortrait = portraitOrientations.has(entry.orientation ?? 0);
  return isPortrait ? "2 / 3" : "3 / 2";
}

export function DayRow({
  dayLabel,
  visibleEntries,
  hiddenImportedCount,
  expanded,
  onToggleExpand,
  selected,
  onTogglePath,
  onSelectAllInDay,
  onSelectNoneInDay,
  loadedThumbs,
  thumbsLoading,
  thumbsLoaded,
}: DayRowProps) {
  const lastClickedIdxRef = useRef<number | null>(null);
  const dayPaths = visibleEntries.map((e) => e.path);
  const selectedInDay = visibleEntries.reduce(
    (n, e) => (selected.has(e.path) ? n + 1 : n),
    0,
  );
  const allSelected = selectedInDay === visibleEntries.length && visibleEntries.length > 0;
  const range = timeRange(visibleEntries);

  return (
    <div
      className="border-b py-3"
      style={{ borderColor: "var(--color-border)" }}
    >
      <button
        type="button"
        onClick={onToggleExpand}
        className="w-full flex items-baseline justify-between text-left cursor-pointer"
      >
        <div className="flex items-baseline gap-2">
          <span
            className="text-[12px]"
            style={{ color: "var(--color-fg-dim)", width: "1ch" }}
          >
            {expanded ? "▼" : "▶"}
          </span>
          <span
            className="text-[14px] font-semibold"
            style={{ color: "var(--color-fg)" }}
          >
            {dayLabel}
          </span>
        </div>
        <div
          className="text-[11px] font-mono flex items-center gap-2"
          style={{ color: "var(--color-fg-dim)" }}
        >
          <span>
            {visibleEntries.length}{" "}
            {visibleEntries.length === 1 ? "photo" : "photos"}
          </span>
          {range && <span>·  {range}</span>}
          {hiddenImportedCount > 0 && (
            <span style={{ color: "var(--color-fg-mute)" }}>
              · {hiddenImportedCount} already imported
            </span>
          )}
        </div>
      </button>

      {expanded && visibleEntries.length > 0 && (
        <div className="mt-3">
          <div
            className="flex items-baseline gap-3 mb-2 text-[11px] font-mono"
            style={{ color: "var(--color-fg-dim)" }}
          >
            <button
              type="button"
              onClick={allSelected ? onSelectNoneInDay : onSelectAllInDay}
              className="flex items-baseline gap-1 cursor-pointer"
              style={{ color: "var(--color-fg)" }}
            >
              <span>{allSelected ? "☑" : "☐"}</span>
              <span>All</span>
            </button>
            <span>
              {selectedInDay} selected
            </span>
            {thumbsLoading && (
              <span style={{ color: "var(--color-fg-mute)" }}>
                · loading thumbnails {thumbsLoaded} / {visibleEntries.length}
              </span>
            )}
          </div>
          <div className="flex flex-wrap gap-1">
            {visibleEntries.map((entry, idx) => {
              const isSelected = selected.has(entry.path);
              const thumb = loadedThumbs.get(entry.path) ?? entry.thumbDataUrl ?? null;
              return (
                <div
                  key={entry.path}
                  onClick={(ev) => {
                    if (ev.shiftKey && lastClickedIdxRef.current != null) {
                      const from = Math.min(lastClickedIdxRef.current, idx);
                      const to = Math.max(lastClickedIdxRef.current, idx);
                      const range = visibleEntries.slice(from, to + 1).map((e) => e.path);
                      onTogglePath(entry.path, ev, range);
                    } else {
                      onTogglePath(entry.path, ev, [entry.path]);
                    }
                    lastClickedIdxRef.current = idx;
                  }}
                  className="relative cursor-pointer overflow-hidden rounded-xs transition-opacity"
                  style={{
                    height: "80px",
                    aspectRatio: aspectFor(entry),
                    background: "var(--color-bg3)",
                    outline: isSelected
                      ? "2px solid var(--color-accent-blue)"
                      : "1px solid var(--color-border)",
                    outlineOffset: isSelected ? "-2px" : "-1px",
                    opacity: isSelected ? 1 : 0.65,
                  }}
                  title={`${entry.filename}${entry.capturedAt ? ` · ${entry.capturedAt}` : ""}`}
                >
                  {thumb ? (
                    <img
                      src={thumb}
                      alt={entry.filename}
                      loading="lazy"
                      draggable={false}
                      className="w-full h-full object-cover"
                    />
                  ) : (
                    <div
                      className="w-full h-full flex items-center justify-center text-[9px] font-mono px-1 text-center break-all"
                      style={{ color: "var(--color-fg-mute)" }}
                    >
                      {entry.filename}
                    </div>
                  )}
                </div>
              );
            })}
            {dayPaths.length === 0 && (
              <div
                className="text-[11px] italic px-1"
                style={{ color: "var(--color-fg-mute)" }}
              >
                Nothing to show in this day.
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
