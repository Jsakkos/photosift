import { useMemo, useCallback, useEffect, useRef } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, Stars } from "../primitives";
import type { DisplayItem } from "../../types";

// Strip width accounts for a permanently-reserved 16px scrollbar
// gutter (so layout doesn't jump when overflow appears) plus 14px
// internal padding. 160 - 30 = 130 visible for the thumbnail.
const STRIP_WIDTH = 160;
const THUMB_W = 124;
const THUMB_H = 82;

/// Categorical hues cycled across clusters so members of one group share
/// a colored edge in the filmstrip. Defined as tokens in globals.css.
export const GROUP_PALETTE = [
  "var(--color-group-1)",
  "var(--color-group-2)",
  "var(--color-group-3)",
  "var(--color-group-4)",
  "var(--color-group-5)",
  "var(--color-group-6)",
  "var(--color-group-7)",
  "var(--color-group-8)",
];

export interface GroupVisual {
  color: string;
  count: number;
}

/// Assign each cluster present in `items` a stable palette color and a
/// visible-member count, so the filmstrip can mark which thumbnails
/// belong together. Groups with fewer than 2 visible members (singletons,
/// or a burst whose siblings were filtered out by the star floor) are
/// omitted — there is no cluster to read. Colors are handed out in
/// encounter order, so adjacent clusters never share a hue until the
/// 8-hue palette wraps.
export function buildGroupInfo(items: DisplayItem[]): Map<number, GroupVisual> {
  const order: number[] = [];
  const counts = new Map<number, number>();
  for (const d of items) {
    if (d.groupId == null) continue;
    if (!counts.has(d.groupId)) order.push(d.groupId);
    counts.set(d.groupId, (counts.get(d.groupId) ?? 0) + 1);
  }

  const info = new Map<number, GroupVisual>();
  let colorIdx = 0;
  for (const gid of order) {
    const count = counts.get(gid)!;
    if (count < 2) continue;
    info.set(gid, {
      color: GROUP_PALETTE[colorIdx % GROUP_PALETTE.length],
      count,
    });
    colorIdx++;
  }
  return info;
}

type Section = {
  rating: 5 | 4 | 3 | 2 | 1 | 0;
  label: string;
  items: DisplayItem[];
};

export function StarGroupedStrip() {
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const setCurrentIndex = useProjectStore((s) => s.setCurrentIndex);
  const setViewMode = useProjectStore((s) => s.setViewMode);
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);
  const activeInnerGroupId = useProjectStore((s) => s.activeInnerGroupId);

  const currentImageId = displayItems[currentIndex]?.image.id ?? null;

  // Cluster → color/count, derived once per display list. Lets the strip
  // mark group membership without disturbing the star-tier bucketing.
  const groupInfo = useMemo(() => buildGroupInfo(displayItems), [displayItems]);

  // Source from displayItems and bucket by star rating so the order
  // within each star tier matches the store's navigation order. Photos
  // filtered out by the current pass level won't appear — they're not
  // in displayItems.
  const sections = useMemo<Section[]>(() => {
    const buckets: Record<0 | 1 | 2 | 3 | 4 | 5, DisplayItem[]> = {
      5: [],
      4: [],
      3: [],
      2: [],
      1: [],
      0: [],
    };
    for (const d of displayItems) {
      const r = Math.max(0, Math.min(5, d.image.starRating)) as 0 | 1 | 2 | 3 | 4 | 5;
      buckets[r].push(d);
    }
    return [
      { rating: 5, label: "★★★★★", items: buckets[5] },
      { rating: 4, label: "★★★★", items: buckets[4] },
      { rating: 3, label: "★★★", items: buckets[3] },
      { rating: 2, label: "★★", items: buckets[2] },
      { rating: 1, label: "★", items: buckets[1] },
      { rating: 0, label: "Unrated", items: buckets[0] },
    ];
  }, [displayItems]);

  const onClick = useCallback(
    (imageId: number) => {
      const idx = displayItems.findIndex((d) => d.image.id === imageId);
      if (idx >= 0) {
        if (activeInnerGroupId !== null) setActiveInnerGroup(null);
        setCurrentIndex(idx);
        setViewMode("sequential");
      }
    },
    [displayItems, activeInnerGroupId, setActiveInnerGroup, setCurrentIndex, setViewMode],
  );

  // Scroll the active thumb into view when arrow-nav crosses a star
  // boundary or moves selection past the strip's visible window.
  const activeRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [currentImageId]);

  return (
    <div
      className="shrink-0 overflow-y-auto"
      style={{
        width: STRIP_WIDTH,
        background: "var(--color-bg)",
        borderRight: "1px solid var(--color-border)",
        scrollbarGutter: "stable",
      }}
    >
      {sections.map((section) => {
        if (section.items.length === 0) return null;
        return (
          <div key={section.rating}>
            <div
              className="sticky top-0 z-10 px-2 py-1.5 flex items-center justify-between"
              style={{
                background: "var(--color-bg2)",
                borderBottom: "1px solid var(--color-border)",
              }}
            >
              {section.rating > 0 ? (
                <Stars value={section.rating} size={9} />
              ) : (
                <span
                  className="font-mono text-3xs uppercase tracking-[0.6px]"
                  style={{ color: "var(--color-fg-mute)" }}
                >
                  no★
                </span>
              )}
              <span
                className="font-mono text-3xs tabular-nums"
                style={{ color: "var(--color-fg-mute)" }}
              >
                {section.items.length}
              </span>
            </div>
            <div className="flex flex-col gap-1 p-[7px]">
              {section.items.map((d) => {
                const img = d.image;
                const active = img.id === currentImageId;
                const group = d.groupId != null ? groupInfo.get(d.groupId) : undefined;
                return (
                  <div
                    key={img.id}
                    ref={active ? activeRef : null}
                    title={group ? `Group of ${group.count}` : undefined}
                  >
                    <Photo
                      src={thumbUrl(img.id)}
                      alt={img.filename}
                      fit="cover"
                      selected={active}
                      groupColor={group?.color}
                      onClick={() => onClick(img.id)}
                      style={{ width: THUMB_W, height: THUMB_H, borderRadius: 2 }}
                    />
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}
