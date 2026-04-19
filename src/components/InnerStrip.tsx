import { useEffect, useRef, useState, useMemo } from "react";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { AiPickBadge } from "./AiPickBadge";

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
          isAiPick: di.isAiPick,
        });
      }
    }
    return entries;
  }, [activeInnerGroupId, displayItems]);

  const highlightIdx = memberEntries.findIndex(
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
        {memberEntries.map((entry, idx) => {
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
        })}
      </div>
    </div>
  );
}
