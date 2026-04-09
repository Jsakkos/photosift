import { useRef, useEffect, useCallback, useState } from "react";
import { FixedSizeList as List } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";

const THUMB_WIDTH = 100;
const THUMB_HEIGHT = 80;
const FILMSTRIP_HEIGHT = THUMB_HEIGHT + 8;

// Thumbnail with retry — background thread may not have generated it yet
function Thumbnail({ imageId, filename }: { imageId: number; filename: string }) {
  const [retryCount, setRetryCount] = useState(0);
  const [loaded, setLoaded] = useState(false);
  const imgRef = useRef<HTMLImageElement>(null);

  // Retry loading every 2 seconds if not loaded (background thread is generating)
  useEffect(() => {
    if (loaded) return;
    const timer = setInterval(() => {
      setRetryCount((c) => c + 1);
    }, 2000);
    // Stop retrying after 3 minutes
    const stop = setTimeout(() => clearInterval(timer), 180000);
    return () => { clearInterval(timer); clearTimeout(stop); };
  }, [loaded]);

  const src = `${thumbUrl(imageId)}${retryCount > 0 ? `?r=${retryCount}` : ""}`;

  return (
    <img
      ref={imgRef}
      src={src}
      alt={filename}
      className={`w-full h-full object-cover ${loaded ? "opacity-100" : "opacity-30"}`}
      loading="lazy"
      draggable={false}
      onLoad={(e) => {
        // Check if it's a real thumbnail (not the 1x1 placeholder)
        const img = e.currentTarget;
        if (img.naturalWidth > 1) {
          setLoaded(true);
        }
      }}
    />
  );
}

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
            <Thumbnail imageId={image.id} filename={image.filename} />
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
        overscanCount={5}
      >
        {ThumbnailItem}
      </List>
    </div>
  );
}
