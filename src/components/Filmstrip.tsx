import { useRef, useEffect, useCallback } from "react";
import { FixedSizeList as List } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";

const THUMB_WIDTH = 100;
const THUMB_HEIGHT = 80;
const FILMSTRIP_HEIGHT = THUMB_HEIGHT + 8;

export function Filmstrip() {
  const { images, currentIndex, setCurrentIndex } = useProjectStore();
  const listRef = useRef<List>(null);

  useEffect(() => {
    if (listRef.current && images.length > 0) {
      listRef.current.scrollToItem(currentIndex, "center");
    }
  }, [currentIndex, images.length]);

  const ThumbnailItem = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const image = images[index];
      if (!image) return null;

      const isCurrent = index === currentIndex;

      return (
        <div
          style={style}
          className="flex items-center justify-center p-1"
          onClick={() => setCurrentIndex(index)}
        >
          <div
            className={`relative cursor-pointer rounded overflow-hidden transition-all ${
              isCurrent
                ? "ring-2 ring-[var(--accent)] brightness-100"
                : "brightness-75 hover:brightness-90"
            }`}
            style={{ width: THUMB_WIDTH - 8, height: THUMB_HEIGHT - 8 }}
          >
            <img
              src={thumbUrl(image.id)}
              alt={image.filename}
              className="w-full h-full object-cover"
              loading="lazy"
              draggable={false}
            />
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
          </div>
        </div>
      );
    },
    [images, currentIndex, setCurrentIndex],
  );

  if (images.length === 0) return null;

  return (
    <div
      className="bg-[var(--bg-secondary)] border-t border-white/10"
      style={{ height: FILMSTRIP_HEIGHT }}
    >
      <List
        ref={listRef}
        height={FILMSTRIP_HEIGHT}
        width={typeof window !== "undefined" ? window.innerWidth : 1400}
        itemCount={images.length}
        itemSize={THUMB_WIDTH}
        layout="horizontal"
        overscanCount={10}
      >
        {ThumbnailItem}
      </List>
    </div>
  );
}
