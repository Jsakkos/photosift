import { useMemo, useCallback } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, Stars } from "../primitives";
import type { ImageEntry } from "../../types";

const STRIP_WIDTH = 148;
const THUMB_H = 82;

export function RatingPeerStrip() {
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const setCurrentIndex = useProjectStore((s) => s.setCurrentIndex);
  const setViewMode = useProjectStore((s) => s.setViewMode);
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);
  const activeInnerGroupId = useProjectStore((s) => s.activeInnerGroupId);

  const current = displayItems[currentIndex] ?? null;
  const currentImageId = current?.image.id ?? null;
  const rating = Math.max(0, Math.min(5, current?.image.starRating ?? 0)) as 0 | 1 | 2 | 3 | 4 | 5;

  // Filter displayItems by the current photo's rating — preserves the
  // store's ordering so arrow-key navigation lands on adjacent peers
  // exactly as they appear in the strip.
  const peers = useMemo<ImageEntry[]>(() => {
    if (!current) return [];
    return displayItems.filter((d) => d.image.starRating === rating).map((d) => d.image);
  }, [displayItems, current, rating]);

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

  return (
    <div
      className="shrink-0 flex flex-col overflow-hidden"
      style={{
        width: STRIP_WIDTH,
        background: "var(--color-bg2)",
        borderRight: "1px solid var(--color-border)",
      }}
    >
      <div
        className="px-[10px] py-[10px] border-b flex items-center justify-between"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="flex items-center gap-2">
          <span className="text-[10px] uppercase tracking-[0.6px]" style={{ color: "var(--color-fg-mute)" }}>
            Rating
          </span>
          {rating > 0 ? (
            <Stars value={rating} size={11} />
          ) : (
            <span className="font-mono text-[10px]" style={{ color: "var(--color-fg-dim)" }}>
              unrated
            </span>
          )}
        </div>
        <span className="font-mono text-[10px] tabular-nums" style={{ color: "var(--color-fg-mute)" }}>
          {peers.length}
        </span>
      </div>
      <div className="flex-1 overflow-y-auto p-[10px] flex flex-col gap-2">
        {peers.map((img) => {
          const active = img.id === currentImageId;
          const photoRating = Math.max(0, Math.min(5, img.starRating)) as 0 | 1 | 2 | 3 | 4 | 5;
          return (
            <Photo
              key={img.id}
              src={thumbUrl(img.id)}
              alt={img.filename}
              fit="cover"
              rating={photoRating}
              selected={active}
              onClick={() => onClick(img.id)}
              style={{ width: "100%", height: THUMB_H, borderRadius: 2 }}
            />
          );
        })}
      </div>
    </div>
  );
}
