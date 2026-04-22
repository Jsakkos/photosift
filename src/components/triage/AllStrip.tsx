import { useRef, useEffect, useLayoutEffect, useState, useMemo, useCallback } from "react";
import { FixedSizeList as List } from "react-window";
import { computeDisplayItems, useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, type Verdict } from "../primitives";
import type { DisplayItem } from "../../types";

// 108px leaves ~16px room for the vertical scrollbar on overflow, so
// thumbs (78 + 14 padding = 92) aren't clipped when the list scrolls.
const STRIP_WIDTH = 108;
const THUMB_W = 78;
const THUMB_H = 52;
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
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);
  const activeInnerGroupId = useProjectStore((s) => s.activeInnerGroupId);

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

  // The flat timeline: every burst is one cover, singletons appear as
  // themselves. Independent of `activeInnerGroupId` so drilling into a
  // group doesn't collapse the strip to just that group's members —
  // the inner TriageGroupStrip owns the drilled-in view.
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

  // When drilled in, highlight the cover of the active group; otherwise
  // highlight whichever flat cell currently holds the focused photo.
  const selectedFlatIndex = useMemo(() => {
    if (activeInnerGroupId !== null) {
      return flatItems.findIndex((d) => d.groupId === activeInnerGroupId);
    }
    if (currentImageId === null) return -1;
    return flatItems.findIndex((d) => d.image.id === currentImageId);
  }, [flatItems, activeInnerGroupId, currentImageId]);

  useEffect(() => {
    if (listRef.current && selectedFlatIndex >= 0) {
      listRef.current.scrollToItem(selectedFlatIndex, "center");
    }
  }, [selectedFlatIndex]);

  const onCellClick = useCallback(
    (item: DisplayItem) => {
      if (item.isGroupCover && item.groupId != null) {
        // Clicking a cover is "switch to this burst" — drill in so the
        // inner strip fills with its members. No-op if we're already
        // drilled into this group.
        if (activeInnerGroupId !== item.groupId) {
          setActiveInnerGroup(item.groupId);
        }
        setViewMode("sequential");
        return;
      }
      // Singleton: close any drill-down first so the clicked photo
      // actually appears in the resulting displayItems, then land on it.
      if (activeInnerGroupId !== null) {
        setActiveInnerGroup(null);
      }
      const fresh = useProjectStore.getState();
      const idx = fresh.displayItems.findIndex((d) => d.image.id === item.image.id);
      if (idx >= 0) fresh.setCurrentIndex(idx);
      setViewMode("sequential");
    },
    [activeInnerGroupId, setActiveInnerGroup, setViewMode],
  );

  const Row = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const item = flatItems[index];
      if (!item) return null;
      const image = item.image;
      const rating = Math.max(0, Math.min(5, image.starRating)) as 0 | 1 | 2 | 3 | 4 | 5;
      return (
        <div style={style} className="px-[7px] py-1">
          <Photo
            src={thumbUrl(image.id)}
            alt={image.filename}
            fit="cover"
            verdict={verdictFromFlag(image.flag)}
            rating={rating}
            groupMember={item.isGroupCover === true}
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
        >
          {Row}
        </List>
      )}
    </div>
  );
}
