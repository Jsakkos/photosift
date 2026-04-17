import { useRef, useEffect, useCallback, useState } from "react";
import { FixedSizeList as List } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { GroupStack } from "./GroupStack";

const THUMB_WIDTH = 100;
const THUMB_HEIGHT = 80;
const FILMSTRIP_HEIGHT = THUMB_HEIGHT + 8;

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
  const { displayItems, currentIndex, setCurrentIndex, currentView, setViewMode, toggleGroupExpansion } = useProjectStore();
  const listRef = useRef<List>(null);

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
            onDoubleClick={() => handleGroupDoubleClick(index, gid)}
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

      return (
        <div
          style={style}
          role="button"
          tabIndex={-1}
          aria-label={image.filename}
          aria-current={isCurrent ? "true" : undefined}
          className="flex items-center justify-center p-1"
          onClick={() => setCurrentIndex(index)}
          onDoubleClick={() => openLoupe(index)}
        >
          <div
            className={`relative cursor-pointer rounded overflow-hidden transition-all ${
              isCurrent
                ? "ring-2 ring-[var(--accent)] brightness-100"
                : "brightness-75 hover:brightness-90"
            }`}
            style={{ width: THUMB_WIDTH - 8, height: THUMB_HEIGHT - 8 }}
          >
            <Thumbnail imageId={image.id} filename={image.filename} />
            {image.flag === "pick" && (
              <div className="absolute top-0.5 left-0.5 w-2.5 h-2.5 rounded-full bg-green-500" />
            )}
            {image.flag === "reject" && (
              <div className="absolute top-0.5 left-0.5 w-2.5 h-2.5 rounded-full bg-red-500" />
            )}
            {image.starRating > 0 && (
              <div className="absolute bottom-0 left-0 right-0 flex justify-center gap-0.5 pb-0.5 bg-gradient-to-t from-black/60 to-transparent">
                {Array.from({ length: image.starRating }, (_, i) => (
                  <div
                    key={i}
                    className="w-1.5 h-1.5 rounded-full bg-[var(--star-filled)]"
                  />
                ))}
              </div>
            )}
            {currentView === "select" && item.groupId && (
              <div className="absolute left-0 top-0 bottom-0 w-0.5 bg-[var(--accent)]" />
            )}
          </div>
        </div>
      );
    },
    [displayItems, currentIndex, setCurrentIndex, currentView, openLoupe, handleGroupDoubleClick],
  );

  if (displayItems.length === 0) return null;

  return (
    <div
      className="bg-[var(--bg-secondary)] border-t border-white/10"
      style={{ height: FILMSTRIP_HEIGHT }}
    >
      <List
        ref={listRef}
        height={FILMSTRIP_HEIGHT}
        width={typeof window !== "undefined" ? window.innerWidth : 1400}
        itemCount={displayItems.length}
        itemSize={THUMB_WIDTH}
        layout="horizontal"
        overscanCount={5}
      >
        {ThumbnailItem}
      </List>
    </div>
  );
}
