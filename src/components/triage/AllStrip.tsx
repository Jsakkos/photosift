import { useRef, useEffect, useLayoutEffect, useState, useMemo, useCallback } from "react";
import { FixedSizeList as List } from "react-window";
import { computeDisplayItems, useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, type StarCount, type Verdict } from "../primitives";
import type { DisplayItem } from "../../types";

// 108px leaves ~16px room for the vertical scrollbar on overflow, so
// thumbs (78 + 14 padding = 92) aren't clipped when the list scrolls.
// Strip width accounts for a permanently-reserved 16px scrollbar
// gutter (so layout doesn't jump when overflow appears) plus 14px row
// padding. 160 - 30 = 130 visible for the thumbnail.
const STRIP_WIDTH = 160;
const THUMB_W = 124;
const THUMB_H = 82;
const CELL_H = THUMB_H + 8;

function verdictFromFlag(flag: string): Verdict {
  if (flag === "pick") return "keep";
  if (flag === "reject") return "toss";
  return null;
}

export function AllStrip() {
  const images = useProjectStore((s) => s.images);
  const groups = useProjectStore((s) => s.groups);
  const showReviewed = useProjectStore((s) => s.showReviewed);
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const setViewMode = useProjectStore((s) => s.setViewMode);

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

  // The triage filmstrip: every passing photo as its own cell, in shoot
  // order. Triage has no group collapsing, so this mirrors `displayItems`
  // — recomputed here only so it stays correct under the Show-all toggle.
  const flatItems = useMemo<DisplayItem[]>(
    () =>
      computeDisplayItems(
        images,
        "triage",
        groups,
        new Set(),
        undefined,
        undefined,
        undefined,
        showReviewed,
      ),
    [images, groups, showReviewed],
  );

  const currentImageId = displayItems[currentIndex]?.image.id ?? null;

  const selectedFlatIndex = useMemo(() => {
    if (currentImageId === null) return -1;
    return flatItems.findIndex((d) => d.image.id === currentImageId);
  }, [flatItems, currentImageId]);

  useEffect(() => {
    if (listRef.current && selectedFlatIndex >= 0) {
      listRef.current.scrollToItem(selectedFlatIndex, "center");
    }
  }, [selectedFlatIndex]);

  const onCellClick = useCallback(
    (item: DisplayItem) => {
      const fresh = useProjectStore.getState();
      const idx = fresh.displayItems.findIndex((d) => d.image.id === item.image.id);
      if (idx >= 0) fresh.setCurrentIndex(idx);
      setViewMode("sequential");
    },
    [setViewMode],
  );

  const Row = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const item = flatItems[index];
      if (!item) return null;
      const image = item.image;
      return (
        <div style={style} className="px-[7px] py-1">
          <Photo
            src={thumbUrl(image.id)}
            alt={image.filename}
            fit="cover"
            verdict={verdictFromFlag(image.flag)}
            stars={image.starRating as StarCount}
            selected={index === selectedFlatIndex}
            dim={image.flag === "reject" ? 0.45 : 1}
            onClick={() => onCellClick(item)}
            style={{ width: THUMB_W, height: THUMB_H, borderRadius: 2 }}
          />
        </div>
      );
    },
    [flatItems, selectedFlatIndex, onCellClick],
  );

  if (flatItems.length === 0) return null;

  return (
    <div
      ref={containerRef}
      className="shrink-0 overflow-hidden"
      style={{
        width: STRIP_WIDTH,
        background: "var(--color-bg)",
        borderRight: "1px solid var(--color-border)",
      }}
    >
      {listHeight > 0 && (
        <List
          ref={listRef}
          height={listHeight}
          width={STRIP_WIDTH}
          itemCount={flatItems.length}
          itemSize={CELL_H}
          layout="vertical"
          overscanCount={6}
          style={{ scrollbarGutter: "stable" }}
        >
          {Row}
        </List>
      )}
    </div>
  );
}
