import { useRef, useEffect, useCallback, useState, useMemo, useLayoutEffect } from "react";
import { VariableSizeList as List } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { GroupStack } from "./GroupStack";
import { GroupTray } from "./GroupTray";
import { AiPickBadge } from "./AiPickBadge";
import { groupTrayPosition } from "../lib/groupTray";

const RAIL_WIDTH = 240;
const THUMB_W = 180;
const THUMB_H = 110;
const BASE_CELL_H = THUMB_H + 8;
const HEADER_ROW_H = 28;

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

export function Filmstrip() {
  const {
    displayItems,
    currentIndex,
    setCurrentIndex,
    currentView,
    setViewMode,
    toggleGroupExpansion,
    groups,
  } = useProjectStore();
  const listRef = useRef<List>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [listHeight, setListHeight] = useState<number>(0);

  // Measure the available height for the list. Parent drives height via
  // flex; we resize-observe to keep the virtualized list in sync on
  // viewport changes.
  useLayoutEffect(() => {
    if (!containerRef.current) return;
    const el = containerRef.current;
    const sync = () => setListHeight(el.clientHeight);
    sync();
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const groupById = useMemo(() => {
    const m = new Map<number, { count: number }>();
    for (const g of groups) m.set(g.id, { count: g.members.length });
    return m;
  }, [groups]);

  const itemSize = useCallback(
    (index: number) => {
      const pos = groupTrayPosition(displayItems, index);
      return pos === "first" || pos === "solo"
        ? BASE_CELL_H + HEADER_ROW_H
        : BASE_CELL_H;
    },
    [displayItems],
  );

  // VariableSizeList caches measured sizes. Reset when the item list
  // changes so cell heights reflect the current group-run structure.
  useEffect(() => {
    listRef.current?.resetAfterIndex(0, false);
  }, [displayItems]);

  const openLoupe = useCallback(
    (index: number) => {
      setCurrentIndex(index);
      setViewMode("sequential");
    },
    [setCurrentIndex, setViewMode],
  );

  const handleGroupDoubleClick = useCallback(
    (index: number, groupId: number) => {
      if (currentView === "triage") {
        setCurrentIndex(index);
        toggleGroupExpansion(groupId);
      } else {
        openLoupe(index);
      }
    },
    [currentView, setCurrentIndex, toggleGroupExpansion, openLoupe],
  );

  useEffect(() => {
    if (listRef.current && displayItems.length > 0) {
      listRef.current.scrollToItem(currentIndex, "center");
    }
  }, [currentIndex, displayItems.length]);

  const ThumbnailItem = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const item = displayItems[index];
      if (!item) return null;

      const image = item.image;
      const isCurrent = index === currentIndex;

      if (currentView === "triage" && item.isGroupCover && item.groupMemberCount && item.groupId !== undefined) {
        const gid = item.groupId;
        return (
          <div
            style={style}
            role="button"
            tabIndex={-1}
            aria-label={`Group cover ${image.filename}, ${item.groupMemberCount} photos`}
            aria-current={isCurrent ? "true" : undefined}
            className="flex items-center justify-center p-1"
            onClick={() => setCurrentIndex(index)}
          >
            <GroupStack
              imageId={image.id}
              filename={image.filename}
              count={item.groupMemberCount}
              isCurrent={isCurrent}
              onClick={() => setCurrentIndex(index)}
              onDoubleClick={() => handleGroupDoubleClick(index, gid)}
              isAiPick={item.isAiPick}
            />
          </div>
        );
      }

      const isExpandedMember =
        currentView === "triage" && item.groupId !== undefined && !item.isGroupCover;
      const onThumbDoubleClick = () => {
        if (isExpandedMember && item.groupId !== undefined) {
          toggleGroupExpansion(item.groupId);
        } else {
          openLoupe(index);
        }
      };

      const trayPos = groupTrayPosition(displayItems, index);
      const inTray = trayPos !== "none";
      const memberCount =
        inTray && item.groupId !== undefined
          ? groupById.get(item.groupId)?.count ?? 0
          : 0;

      const thumbBlock = (
        <div
          role="button"
          tabIndex={-1}
          aria-label={image.filename}
          aria-current={isCurrent ? "true" : undefined}
          className="flex items-center justify-center"
          onClick={() => setCurrentIndex(index)}
          onDoubleClick={onThumbDoubleClick}
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

      if (inTray) {
        return (
          <div style={style} className="px-2">
            <GroupTray position={trayPos} memberCount={memberCount}>
              {thumbBlock}
            </GroupTray>
          </div>
        );
      }

      // Standalone photo: Select-view group affiliation stripe still
      // applies (photos that belong to a group but aren't being rendered
      // in a tray because they're a cover handled above, or the view
      // isn't triage). Keep the original 3px left bar treatment.
      const showSelectBar =
        currentView === "select" && item.groupId !== undefined && !item.isGroupCover;

      return (
        <div
          style={style}
          role="button"
          tabIndex={-1}
          aria-label={image.filename}
          aria-current={isCurrent ? "true" : undefined}
          className="flex items-center justify-center p-1"
          onClick={() => setCurrentIndex(index)}
          onDoubleClick={onThumbDoubleClick}
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
            {showSelectBar && (
              <div className="absolute left-0 top-0 bottom-0 w-[3px] bg-[var(--accent)]" />
            )}
          </div>
        </div>
      );
    },
    [displayItems, currentIndex, setCurrentIndex, currentView, openLoupe, handleGroupDoubleClick, toggleGroupExpansion, groupById],
  );

  if (displayItems.length === 0) return null;

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
          itemCount={displayItems.length}
          itemSize={itemSize}
          layout="vertical"
          overscanCount={5}
        >
          {ThumbnailItem}
        </List>
      )}
    </div>
  );
}
