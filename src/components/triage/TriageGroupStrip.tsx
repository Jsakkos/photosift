import { useMemo, useCallback, useEffect, useRef } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, type Verdict } from "../primitives";
import type { ImageEntry } from "../../types";

// Strip width includes a permanently-reserved 16px scrollbar gutter
// so thumbnails don't reflow when the strip overflows. 200 - 16 - 20
// (inner padding) = 164px for each thumbnail.
const STRIP_WIDTH = 200;

// Orientation-aware thumb aspect. EXIF 5-8 means the image was rotated
// 90°/270° at ingest so its displayed aspect is flipped.
function aspectFor(img: ImageEntry): string {
  const o = img.orientation ?? 1;
  return o >= 5 && o <= 8 ? "2 / 3" : "3 / 2";
}

function verdictFromFlag(flag: string): Verdict {
  if (flag === "pick") return "keep";
  if (flag === "reject") return "toss";
  return null;
}

export function TriageGroupStrip() {
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const images = useProjectStore((s) => s.images);
  const groups = useProjectStore((s) => s.groups);
  const setCurrentIndex = useProjectStore((s) => s.setCurrentIndex);
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);
  const setViewMode = useProjectStore((s) => s.setViewMode);

  const current = displayItems[currentIndex] ?? null;
  const currentImageId = current?.image.id ?? null;

  const group = useMemo(() => {
    if (!current) return null;
    if (current.groupId !== undefined) {
      return groups.find((g) => g.id === current.groupId) ?? null;
    }
    return groups.find((g) => g.members.some((m) => m.photoId === current.image.id)) ?? null;
  }, [current, groups]);

  // Sort group members by quality score (best first), matching the
  // store's in-group ordering from computeDisplayItems — so clicking
  // right arrow in the loupe lands on the next thumb down the strip.
  const members = useMemo(() => {
    if (!group) return [] as ImageEntry[];
    const byId = new Map(images.map((i) => [i.id, i] as const));
    const ordered = group.members
      .map((m) => byId.get(m.photoId))
      .filter((i): i is ImageEntry => i !== undefined);
    ordered.sort((a, b) => {
      const aq = typeof a.qualityScore === "number" ? a.qualityScore : -Infinity;
      const bq = typeof b.qualityScore === "number" ? b.qualityScore : -Infinity;
      if (aq !== bq) return bq - aq;
      return a.id - b.id;
    });
    return ordered;
  }, [group, images]);

  const onClick = useCallback(
    (imageId: number) => {
      setViewMode("sequential");
      // First, see if the clicked member is already in the current
      // displayItems (drilled-in case): just navigate.
      const idx = displayItems.findIndex((d) => d.image.id === imageId);
      if (idx >= 0) {
        setCurrentIndex(idx);
        return;
      }
      // Otherwise we're collapsed and the user clicked a non-cover
      // member. Drill into this group, then snap focus to the clicked
      // member. The previous code called setActiveInnerGroup(null)
      // unconditionally and then setCurrentIndex(idx) using an `idx`
      // computed against the pre-collapse displayItems — which after
      // the collapse pointed at a totally unrelated photo. This path
      // expands instead and re-resolves the index against the
      // post-expand list.
      if (group) {
        setActiveInnerGroup(group.id);
        const fresh = useProjectStore.getState().displayItems;
        const newIdx = fresh.findIndex((d) => d.image.id === imageId);
        if (newIdx >= 0) setCurrentIndex(newIdx);
      }
    },
    [displayItems, group, setActiveInnerGroup, setCurrentIndex, setViewMode],
  );

  // Scroll the active thumb into view as arrow-nav moves the focus off
  // screen. `block: "nearest"` avoids jitter when the thumb is already
  // visible.
  const activeRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [currentImageId]);

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
        className="flex-1 overflow-y-auto p-2.5 flex flex-col gap-2"
        style={{ scrollbarGutter: "stable" }}
      >
        {members.length === 0 && current && (
          <Photo
            src={thumbUrl(current.image.id)}
            alt={current.image.filename}
            fit="cover"
            verdict={verdictFromFlag(current.image.flag)}
            selected
            style={{ width: "100%", aspectRatio: aspectFor(current.image), borderRadius: 2 }}
          />
        )}
        {members.map((img) => {
          const active = img.id === currentImageId;
          return (
            <div key={img.id} ref={active ? activeRef : null}>
              <Photo
                src={thumbUrl(img.id)}
                alt={img.filename}
                fit="cover"
                verdict={verdictFromFlag(img.flag)}
                selected={active}
                dim={img.flag === "reject" && !active ? 0.5 : 1}
                onClick={() => onClick(img.id)}
                style={{ width: "100%", aspectRatio: aspectFor(img), borderRadius: 2 }}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
