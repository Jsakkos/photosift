import { useEffect, useRef, useState, useMemo } from "react";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { AiPickBadge } from "./AiPickBadge";
import { computeGroupRanks, rankColorClass } from "../lib/groupRanking";

const STRIP_WIDTH = 200;
const THUMB_W = 160;
const THUMB_H = 100;

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
/// group's members. Narrower than the outer rail so the reading order
/// flows left-to-right: outer → inner → loupe. Native scroll — groups
/// rarely exceed a few hundred members so virtualization isn't worth
/// the layout-measurement complexity.
export function InnerStrip() {
  const activeInnerGroupId = useProjectStore((s) => s.activeInnerGroupId);
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const groups = useProjectStore((s) => s.groups);
  const setCurrentIndex = useProjectStore((s) => s.setCurrentIndex);
  const setViewMode = useProjectStore((s) => s.setViewMode);
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);
  const scrollRef = useRef<HTMLDivElement>(null);

  const memberEntries = useMemo(() => {
    if (activeInnerGroupId == null) return [];
    const entries: {
      displayIndex: number;
      imageId: number;
      filename: string;
      flag: string;
      starRating: number;
      qualityScore: number | null | undefined;
      faceCount: number | null | undefined;
      eyesOpenCount: number | null | undefined;
      isAiPick?: boolean;
    }[] = [];
    for (let i = 0; i < displayItems.length; i++) {
      const di = displayItems[i];
      if (di.groupId === activeInnerGroupId && !di.isGroupCover) {
        entries.push({
          displayIndex: i,
          imageId: di.image.id,
          filename: di.image.filename,
          flag: di.image.flag,
          starRating: di.image.starRating,
          qualityScore: di.image.qualityScore,
          faceCount: di.image.faceCount,
          eyesOpenCount: di.image.eyesOpenCount,
          isAiPick: di.isAiPick,
        });
      }
    }
    return entries;
  }, [activeInnerGroupId, displayItems]);


  const ranks = useMemo(
    () =>
      computeGroupRanks(
        memberEntries.map((e) => ({ id: e.imageId, qualityScore: e.qualityScore })),
      ),
    [memberEntries],
  );

  // Sort by quality (best-first) inside the group so the user sees the
  // AI pick and runners-up at the top of the strip instead of in
  // capture-time order. Fall back to capture-time (displayIndex) when
  // rank is null (unanalyzed or single-member groups), and as a stable
  // tiebreaker within a given rank.
  const sortedMembers = useMemo(() => {
    const copy = [...memberEntries];
    copy.sort((a, b) => {
      const ra = ranks.get(a.imageId)?.rank;
      const rb = ranks.get(b.imageId)?.rank;
      if (ra != null && rb != null) {
        if (ra !== rb) return ra - rb;
      } else if (ra != null) {
        return -1;
      } else if (rb != null) {
        return 1;
      }
      return a.displayIndex - b.displayIndex;
    });
    return copy;
  }, [memberEntries, ranks]);

  const highlightIdx = sortedMembers.findIndex(
    (m) => m.displayIndex === currentIndex,
  );

  // Scroll the selected member into view whenever the selection changes.
  useEffect(() => {
    if (highlightIdx < 0) return;
    const scroller = scrollRef.current;
    const target = scroller?.children[highlightIdx] as HTMLElement | undefined;
    target?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [highlightIdx]);

  if (activeInnerGroupId == null) return null;
  if (memberEntries.length === 0) return null;

  const activeGroup = groups.find((g) => g.id === activeInnerGroupId) ?? null;

  return (
    <div
      className="bg-[var(--accent)]/5 border-r border-[var(--accent)]/40 flex-shrink-0 flex flex-col"
      style={{ width: STRIP_WIDTH }}
      role="region"
      aria-label={`Group ${activeInnerGroupId} members`}
    >
      <div className="px-2 py-1.5 border-b border-[var(--accent)]/30 flex items-center justify-between flex-shrink-0">
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
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center gap-2 py-2"
      >
        {sortedMembers.map((entry, idx) => {
          const isCurrent = idx === highlightIdx;
          return (
            <div
              key={entry.imageId}
              role="button"
              tabIndex={-1}
              aria-label={entry.filename}
              aria-current={isCurrent ? "true" : undefined}
              className="flex-shrink-0 cursor-pointer"
              onClick={() => setCurrentIndex(entry.displayIndex)}
              onDoubleClick={() => {
                setCurrentIndex(entry.displayIndex);
                setViewMode("sequential");
              }}
            >
              <div
                className={`relative rounded overflow-hidden transition-all ${
                  isCurrent
                    ? "ring-2 ring-[var(--accent)] brightness-100"
                    : entry.flag === "pick"
                      ? "ring-2 ring-green-500/70 brightness-90 hover:brightness-95"
                      : entry.flag === "reject"
                        ? "ring-2 ring-red-500/50 brightness-60 hover:brightness-70"
                        : "brightness-75 hover:brightness-90"
                }`}
                style={{ width: THUMB_W, height: THUMB_H }}
              >
                <Thumbnail imageId={entry.imageId} filename={entry.filename} />
                {(() => {
                  const r = ranks.get(entry.imageId);
                  const cls = rankColorClass(r?.color ?? null);
                  if (!cls || r == null || r.rank == null) return null;
                  const total = memberEntries.length;
                  const q = entry.qualityScore;
                  const qPart =
                    typeof q === "number" ? ` \u00b7 quality ${q.toFixed(0)}/100` : "";
                  const tooltip =
                    `Rank ${r.rank + 1} of ${total}${qPart}
Within-group ranking by composite AI quality score.
Green top third \u00b7 white middle \u00b7 red bottom third.`;
                  return (
                    <div
                      className={`absolute bottom-1 right-1 ${cls} text-black/80 text-[10px] font-semibold leading-none rounded px-1 py-0.5 shadow-sm pointer-events-auto`}
                      title={tooltip}
                      aria-label={`Rank ${r.rank + 1} of ${total}`}
                    >
                      {`#${r.rank + 1}`}
                    </div>
                  );
                })()}
                {entry.isAiPick && <AiPickBadge />}
                {entry.starRating > 0 && (
                  <div
                    className="absolute bottom-0 left-0 right-0 flex justify-center gap-0.5 pb-1 bg-gradient-to-t from-black/60 to-transparent pointer-events-auto"
                    title={`${entry.starRating} of 5 stars\nPress 1-5 to rate, 0 to clear.`}
                    aria-label={`${entry.starRating} of 5 stars`}
                  >
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
        })}
      </div>
    </div>
  );
}
