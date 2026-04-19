import { useRef, useEffect, useCallback, useState, useLayoutEffect } from "react";
import { FixedSizeList as List } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { AiPickBadge } from "./AiPickBadge";

const STRIP_WIDTH = 180;
const THUMB_W = 140;
const THUMB_H = 90;
const CELL_H = THUMB_H + 10;

function Thumbnail({ imageId, filename }: { imageId: number; filename: string }) {
  const [loaded, setLoaded] = useState(false);
  return (
    <img
      src={thumbUrl(imageId)}
      alt={filename}
      className={`w-full h-full object-cover ${loaded ? "opacity-100" : "opacity-30"}`}
      loading="lazy"
      draggable={false}
      onLoad={(e) => {
        if (e.currentTarget.naturalWidth > 1) setLoaded(true);
      }}
    />
  );
}

/// The **inner** strip: a second vertical rail that appears when a
/// group is drilled in via `setActiveInnerGroup`. Shows only that
/// group's members. Narrower than the outer rail so the user reads
/// outer → inner → loupe left-to-right. Clicking anywhere else in the
/// outer rail (or pressing Esc) closes it.
export function InnerStrip() {
  const activeInnerGroupId = useProjectStore((s) => s.activeInnerGroupId);
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const groups = useProjectStore((s) => s.groups);
  const setCurrentIndex = useProjectStore((s) => s.setCurrentIndex);
  const setViewMode = useProjectStore((s) => s.setViewMode);
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);

  const listRef = useRef<List>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [listHeight, setListHeight] = useState(0);

  useLayoutEffect(() => {
    if (!containerRef.current) return;
    const el = containerRef.current;
    const sync = () => setListHeight(el.clientHeight);
    sync();
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Members of the active group that are currently in displayItems
  // (flag filters already applied). We map to their display-index so
  // selection flows through the store.
  const memberEntries: { displayIndex: number; imageId: number; filename: string; flag: string; starRating: number; isAiPick?: boolean }[] = [];
  if (activeInnerGroupId != null) {
    for (let i = 0; i < displayItems.length; i++) {
      const di = displayItems[i];
      if (di.groupId === activeInnerGroupId && !di.isGroupCover) {
        memberEntries.push({
          displayIndex: i,
          imageId: di.image.id,
          filename: di.image.filename,
          flag: di.image.flag,
          starRating: di.image.starRating,
          isAiPick: di.isAiPick,
        });
      }
    }
  }

  const highlightIdx = memberEntries.findIndex((m) => m.displayIndex === currentIndex);

  const onCellClick = useCallback(
    (idx: number) => {
      const entry = memberEntries[idx];
      if (entry) setCurrentIndex(entry.displayIndex);
    },
    [memberEntries, setCurrentIndex],
  );

  const onCellDoubleClick = useCallback(
    (idx: number) => {
      const entry = memberEntries[idx];
      if (entry) {
        setCurrentIndex(entry.displayIndex);
        setViewMode("sequential");
      }
    },
    [memberEntries, setCurrentIndex, setViewMode],
  );

  useEffect(() => {
    if (listRef.current && memberEntries.length > 0 && highlightIdx >= 0) {
      listRef.current.scrollToItem(highlightIdx, "center");
    }
  }, [highlightIdx, memberEntries.length]);

  const activeGroup = activeInnerGroupId != null ? groups.find((g) => g.id === activeInnerGroupId) : null;

  const Cell = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const entry = memberEntries[index];
      if (!entry) return null;
      const isCurrent = index === highlightIdx;
      return (
        <div
          style={style}
          role="button"
          tabIndex={-1}
          aria-label={entry.filename}
          aria-current={isCurrent ? "true" : undefined}
          className="flex items-center justify-center p-1"
          onClick={() => onCellClick(index)}
          onDoubleClick={() => onCellDoubleClick(index)}
        >
          <div
            className={`relative cursor-pointer rounded overflow-hidden transition-all ${
              isCurrent
                ? "ring-2 ring-[var(--accent)] brightness-100"
                : "brightness-75 hover:brightness-90"
            }`}
            style={{ width: THUMB_W, height: THUMB_H }}
          >
            <Thumbnail imageId={entry.imageId} filename={entry.filename} />
            {entry.flag === "pick" && (
              <div className="absolute top-1 left-1 w-3 h-3 rounded-full bg-green-500" />
            )}
            {entry.flag === "reject" && (
              <div className="absolute top-1 left-1 w-3 h-3 rounded-full bg-red-500" />
            )}
            {entry.isAiPick && <AiPickBadge />}
            {entry.starRating > 0 && (
              <div className="absolute bottom-0 left-0 right-0 flex justify-center gap-0.5 pb-1 bg-gradient-to-t from-black/60 to-transparent">
                {Array.from({ length: entry.starRating }, (_, i) => (
                  <div
                    key={i}
                    className="w-1.5 h-1.5 rounded-full bg-[var(--star-filled)]"
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      );
    },
    [memberEntries, highlightIdx, onCellClick, onCellDoubleClick],
  );

  if (activeInnerGroupId == null) return null;
  if (memberEntries.length === 0) return null;

  return (
    <div
      ref={containerRef}
      className="bg-[var(--accent)]/5 border-r border-[var(--accent)]/40 flex-shrink-0 flex flex-col"
      style={{ width: STRIP_WIDTH }}
      role="region"
      aria-label={`Group ${activeInnerGroupId} members`}
    >
      <div className="px-2 py-1.5 border-b border-[var(--accent)]/30 flex items-center justify-between">
        <span className="text-[10px] font-semibold uppercase tracking-wide text-[var(--accent)]">
          Group · {activeGroup?.members.length ?? memberEntries.length}
        </span>
        <button
          type="button"
          onClick={() => setActiveInnerGroup(null)}
          aria-label="Close inner strip"
          className="text-white/50 hover:text-white/90 text-xs leading-none px-1 py-0.5 rounded"
          title="Close (Esc)"
        >
          ×
        </button>
      </div>
      <div className="flex-1">
        {listHeight > 0 && (
          <List
            ref={listRef}
            height={listHeight - 32}
            width={STRIP_WIDTH}
            itemCount={memberEntries.length}
            itemSize={CELL_H}
            layout="vertical"
            overscanCount={5}
          >
            {Cell}
          </List>
        )}
      </div>
    </div>
  );
}
