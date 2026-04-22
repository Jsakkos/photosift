import { useMemo, useCallback } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, type Verdict } from "../primitives";
import type { ImageEntry, Group } from "../../types";

const STRIP_WIDTH = 148;
const THUMB_H = 82;

function verdictFromFlag(flag: string): Verdict {
  if (flag === "pick") return "keep";
  if (flag === "reject") return "toss";
  return null;
}

function groupOrdinalFor(groups: Group[], groupId: number): number {
  const ordered = [...groups].sort((a, b) => a.id - b.id);
  const idx = ordered.findIndex((g) => g.id === groupId);
  return idx >= 0 ? idx + 1 : 0;
}

function formatBurstDuration(members: ImageEntry[]): string | null {
  const times = members
    .map((m) => (m.captureTime ? Date.parse(m.captureTime) : NaN))
    .filter((t) => !Number.isNaN(t));
  if (times.length < 2) return null;
  const span = (Math.max(...times) - Math.min(...times)) / 1000;
  if (span < 1) return `${(span * 1000).toFixed(0)}ms`;
  if (span < 60) return `${span.toFixed(1)}s`;
  const minutes = Math.floor(span / 60);
  const seconds = Math.round(span % 60);
  return `${minutes}m ${seconds}s`;
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

  const ordinal = group ? groupOrdinalFor(groups, group.id) : 0;
  const burst = formatBurstDuration(members);

  const onClick = useCallback(
    (imageId: number) => {
      const idx = displayItems.findIndex((d) => d.image.id === imageId);
      if (idx >= 0) {
        setActiveInnerGroup(null);
        setCurrentIndex(idx);
        setViewMode("sequential");
      }
    },
    [displayItems, setActiveInnerGroup, setCurrentIndex, setViewMode],
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
        className="px-[10px] py-[10px] border-b"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="text-[11px] font-medium" style={{ color: "var(--color-fg)" }}>
          {group ? `Group G${ordinal}` : "No group"}
        </div>
        <div
          className="font-mono text-[10px] mt-[2px]"
          style={{ color: "var(--color-fg-mute)" }}
        >
          {group ? `${members.length} photos` : "single frame"}
          {burst && (
            <>
              <span className="opacity-50"> · </span>
              <span>{burst} burst</span>
            </>
          )}
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-[10px] flex flex-col gap-2">
        {members.length === 0 && current && (
          <Photo
            src={thumbUrl(current.image.id)}
            alt={current.image.filename}
            fit="cover"
            verdict={verdictFromFlag(current.image.flag)}
            selected
            style={{ width: "100%", height: THUMB_H, borderRadius: 2 }}
          />
        )}
        {members.map((img) => {
          const active = img.id === currentImageId;
          const rating = Math.max(0, Math.min(5, img.starRating)) as 0 | 1 | 2 | 3 | 4 | 5;
          return (
            <Photo
              key={img.id}
              src={thumbUrl(img.id)}
              alt={img.filename}
              fit="cover"
              verdict={verdictFromFlag(img.flag)}
              rating={rating}
              selected={active}
              dim={img.flag === "reject" && !active ? 0.5 : 1}
              onClick={() => onClick(img.id)}
              style={{ width: "100%", height: THUMB_H, borderRadius: 2 }}
            />
          );
        })}
      </div>
    </div>
  );
}
