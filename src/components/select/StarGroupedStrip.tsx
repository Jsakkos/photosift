import { useMemo, useCallback, useEffect, useRef } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, Stars } from "../primitives";
import type { ImageEntry } from "../../types";

// Strip width accounts for a permanently-reserved 16px scrollbar
// gutter (so layout doesn't jump when overflow appears) plus 14px
// internal padding. 160 - 30 = 130 visible for the thumbnail.
const STRIP_WIDTH = 160;
const THUMB_W = 124;
const THUMB_H = 82;

type Section = {
  rating: 5 | 4 | 3 | 2 | 1 | 0;
  label: string;
  items: ImageEntry[];
};

export function StarGroupedStrip() {
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const setCurrentIndex = useProjectStore((s) => s.setCurrentIndex);
  const setViewMode = useProjectStore((s) => s.setViewMode);
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);
  const activeInnerGroupId = useProjectStore((s) => s.activeInnerGroupId);

  const currentImageId = displayItems[currentIndex]?.image.id ?? null;

  // Source from displayItems and bucket by star rating so the order
  // within each star tier matches the store's navigation order. Photos
  // filtered out by the current pass level won't appear â€” they're not
  // in displayItems.
  const sections = useMemo<Section[]>(() => {
    const buckets: Record<0 | 1 | 2 | 3 | 4 | 5, ImageEntry[]> = {
      5: [],
      4: [],
      3: [],
      2: [],
      1: [],
      0: [],
    };
    for (const d of displayItems) {
      const r = Math.max(0, Math.min(5, d.image.starRating)) as 0 | 1 | 2 | 3 | 4 | 5;
      buckets[r].push(d.image);
    }
    return [
      { rating: 5, label: "â˜…â˜…â˜…â˜…â˜…", items: buckets[5] },
      { rating: 4, label: "â˜…â˜…â˜…â˜…", items: buckets[4] },
      { rating: 3, label: "â˜…â˜…â˜…", items: buckets[3] },
      { rating: 2, label: "â˜…â˜…", items: buckets[2] },
      { rating: 1, label: "â˜…", items: buckets[1] },
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
                  noâ˜…
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
              {section.items.map((img) => {
                const active = img.id === currentImageId;
                return (
                  <div key={img.id} ref={active ? activeRef : null}>
                    <Photo
                      src={thumbUrl(img.id)}
                      alt={img.filename}
                      fit="cover"
                      selected={active}
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
