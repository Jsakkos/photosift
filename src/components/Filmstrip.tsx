import { useRef, useEffect, useCallback, useState, useMemo, useLayoutEffect } from "react";
import { FixedSizeList as List } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { GroupStack } from "./GroupStack";
import { AiPickBadge } from "./AiPickBadge";
import { computeDisplayItems } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useAiStore } from "../stores/aiStore";

const RAIL_WIDTH = 240;
const THUMB_W = 180;
const THUMB_H = 110;
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

/// The **outer** rail: always shows a groups-collapsed view of every
/// eligible photo. Double-click a group cover to open the InnerStrip
/// for that group. Selection here tracks the user's position in the
/// outer list — when the InnerStrip is open, navigation flows through
/// the inner-group members instead (driven by `displayItems`).
export function Filmstrip() {
  const images = useProjectStore((s) => s.images);
  const groups = useProjectStore((s) => s.groups);
  const currentView = useProjectStore((s) => s.currentView);
  const displayItems = useProjectStore((s) => s.displayItems);
  const currentIndex = useProjectStore((s) => s.currentIndex);
  const activeInnerGroupId = useProjectStore((s) => s.activeInnerGroupId);
  const sortByAi = useProjectStore((s) => s.sortByAi);
  const setCurrentIndex = useProjectStore((s) => s.setCurrentIndex);
  const setViewMode = useProjectStore((s) => s.setViewMode);
  const setActiveInnerGroup = useProjectStore((s) => s.setActiveInnerGroup);
  const settings = useSettingsStore((s) => s.settings);
  const eyeProvider = useAiStore((s) => s.eyeProvider);

  const listRef = useRef<List>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [listHeight, setListHeight] = useState<number>(0);

  useLayoutEffect(() => {
    if (!containerRef.current) return;
    const el = containerRef.current;
    const sync = () => setListHeight(el.clientHeight);
    sync();
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Outer-rail items: always-collapsed, regardless of active inner
  // group. Derived locally so the store's `displayItems` can remain the
  // loupe/keyboard cycle target (which *does* change when drilled in).
  const outerItems = useMemo(() => {
    const aiOptions = {
      sortByAi,
      hideSoftThreshold: settings.hideSoftThreshold ?? 0,
      useEyesInPick: eyeProvider === "onnx",
    };
    return computeDisplayItems(
      images,
      currentView,
      groups,
      new Set<number>(),
      settings.selectRequiresPick ?? false,
      settings.routeMinStar ?? 0,
      aiOptions,
    );
  }, [
    images,
    groups,
    currentView,
    settings.selectRequiresPick,
    settings.routeMinStar,
    settings.hideSoftThreshold,
    sortByAi,
    eyeProvider,
  ]);

  // The outer rail's "highlighted" index:
  //  - If no inner strip is open, use the store's currentIndex (which
  //    indexes into `outerItems`).
  //  - If drilled in, surface the active group's cover so the user
  //    sees where they are in the outer view. The group cover always
  //    carries `isGroupCover: true` plus a matching groupId.
  const highlightIndex = useMemo(() => {
    if (activeInnerGroupId == null) return currentIndex;
    return outerItems.findIndex(
      (di) => di.isGroupCover && di.groupId === activeInnerGroupId,
    );
  }, [activeInnerGroupId, outerItems, currentIndex]);

  const openLoupe = useCallback(
    (outerIdx: number) => {
      // Clicking an outer thumb while drilled in: exit the inner strip,
      // then align the store's index to the outer item.
      if (activeInnerGroupId != null) setActiveInnerGroup(null);
      // `setCurrentIndex` works against `displayItems`, which after the
      // inner-group clear will equal `outerItems`.
      setCurrentIndex(outerIdx);
      setViewMode("sequential");
    },
    [activeInnerGroupId, setActiveInnerGroup, setCurrentIndex, setViewMode],
  );

  const onCellClick = useCallback(
    (outerIdx: number) => {
      const item = outerItems[outerIdx];
      if (!item) return;

      if (item.isGroupCover && item.groupId !== undefined &&
          (currentView === "triage" || currentView === "select")) {
        // Single-click on a group cover expands it into the inner
        // strip. `setActiveInnerGroup` is non-toggling, so repeated
        // single-clicks on the same cover are no-ops — contracting
        // happens on double-click.
        setCurrentIndex(outerIdx);
        setActiveInnerGroup(item.groupId);
        return;
      }

      // Non-cover cell clicked. If drilled in, close the inner strip so
      // the loupe cycle realigns to the outer selection.
      if (activeInnerGroupId != null) {
        setActiveInnerGroup(null);
      }
      const idx = displayItems.findIndex((d) => d.image.id === item.image.id);
      setCurrentIndex(idx >= 0 ? idx : outerIdx);
    },
    [outerItems, displayItems, activeInnerGroupId, currentView, setActiveInnerGroup, setCurrentIndex],
  );

  const onCoverDoubleClick = useCallback(
    (_groupId: number, outerIdx: number) => {
      if (currentView === "triage" || currentView === "select") {
        // Double-click contracts the inner strip. The preceding
        // single-click already expanded it; double-click is the
        // "go back" gesture. User confirmed this is the less-common
        // action, so we dedicate the slower gesture to it.
        setActiveInnerGroup(null);
      } else {
        openLoupe(outerIdx);
      }
    },
    [currentView, setActiveInnerGroup, openLoupe],
  );

  useEffect(() => {
    if (listRef.current && outerItems.length > 0 && highlightIndex >= 0) {
      listRef.current.scrollToItem(highlightIndex, "center");
    }
  }, [highlightIndex, outerItems.length]);

  const ThumbnailItem = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const item = outerItems[index];
      if (!item) return null;

      const image = item.image;
      const isCurrent = index === highlightIndex;

      if (item.isGroupCover && item.groupMemberCount && item.groupId !== undefined) {
        const gid = item.groupId;
        return (
          <div
            style={style}
            role="button"
            tabIndex={-1}
            aria-label={`Group cover ${image.filename}, ${item.groupMemberCount} photos`}
            aria-current={isCurrent ? "true" : undefined}
            className="flex items-center justify-center p-1"
            onClick={() => onCellClick(index)}
          >
            <GroupStack
              imageId={image.id}
              filename={image.filename}
              count={item.groupMemberCount}
              isCurrent={isCurrent}
              onClick={() => onCellClick(index)}
              onDoubleClick={() => onCoverDoubleClick(gid, index)}
              isAiPick={item.isAiPick}
              coverW={THUMB_W - 8}
              coverH={THUMB_H - 8}
            />
          </div>
        );
      }

      return (
        <div
          style={style}
          role="button"
          tabIndex={-1}
          aria-label={image.filename}
          aria-current={isCurrent ? "true" : undefined}
          className="flex items-center justify-center p-1"
          onClick={() => onCellClick(index)}
          onDoubleClick={() => openLoupe(index)}
        >
          <div
            className={`relative cursor-pointer rounded overflow-hidden transition-all ${
              isCurrent
                ? "ring-2 ring-[var(--accent)] brightness-100"
                : "brightness-75 hover:brightness-90"
            }`}
            style={{ width: THUMB_W, height: THUMB_H }}
          >
            <Thumbnail imageId={image.id} filename={image.filename} />
            {image.flag === "pick" && (
              <div className="absolute top-1 left-1 w-3 h-3 rounded-full bg-green-500" />
            )}
            {image.flag === "reject" && (
              <div className="absolute top-1 left-1 w-3 h-3 rounded-full bg-red-500" />
            )}
            {item.isAiPick && <AiPickBadge />}
            {image.starRating > 0 && (
              <div className="absolute bottom-0 left-0 right-0 flex justify-center gap-0.5 pb-1 bg-gradient-to-t from-black/60 to-transparent">
                {Array.from({ length: image.starRating }, (_, i) => (
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
    [outerItems, highlightIndex, onCellClick, onCoverDoubleClick, openLoupe],
  );

  if (outerItems.length === 0) return null;

  return (
    <div
      ref={containerRef}
      className="bg-[var(--bg-secondary)] border-r border-white/10 flex-shrink-0"
      style={{ width: RAIL_WIDTH }}
    >
      {listHeight > 0 && (
        <List
          ref={listRef}
          height={listHeight}
          width={RAIL_WIDTH}
          itemCount={outerItems.length}
          itemSize={CELL_H}
          layout="vertical"
          overscanCount={5}
        >
          {ThumbnailItem}
        </List>
      )}
    </div>
  );
}
