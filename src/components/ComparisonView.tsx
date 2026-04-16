import { useRef, useState, useCallback } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useImageLoader, thumbUrl } from "../hooks/useImageLoader";

export function ComparisonView() {
  const {
    comparisonPinnedId,
    comparisonCyclingId,
    comparisonGroupMembers,
    images,
  } = useProjectStore();

  const pinnedImage = images.find((i) => i.id === comparisonPinnedId) ?? null;
  const cyclingImage = images.find((i) => i.id === comparisonCyclingId) ?? null;

  const { displayUrl: pinnedUrl } = useImageLoader(comparisonPinnedId);
  const { displayUrl: cyclingUrl } = useImageLoader(comparisonCyclingId);

  const [transform, setTransform] = useState({ x: 0, y: 0, scale: 1 });
  const isDragging = useRef(false);
  const dragStart = useRef({ x: 0, y: 0 });

  const availableMembers = comparisonGroupMembers.filter((id) => {
    const img = images.find((i) => i.id === id);
    return img && img.flag !== "reject";
  });
  const cyclingIdx = availableMembers.indexOf(comparisonCyclingId!);

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      const delta = e.deltaY > 0 ? 0.9 : 1.1;
      setTransform((t) => ({
        ...t,
        scale: Math.max(0.5, Math.min(10, t.scale * delta)),
      }));
    },
    [],
  );

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (transform.scale <= 1) return;
      isDragging.current = true;
      dragStart.current = {
        x: e.clientX - transform.x,
        y: e.clientY - transform.y,
      };
    },
    [transform],
  );

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging.current) return;
    setTransform((t) => ({
      ...t,
      x: e.clientX - dragStart.current.x,
      y: e.clientY - dragStart.current.y,
    }));
  }, []);

  const handleMouseUp = useCallback(() => {
    isDragging.current = false;
  }, []);

  const imgStyle: React.CSSProperties = {
    transform: `scale(${transform.scale}) translate(${transform.x / transform.scale}px, ${transform.y / transform.scale}px)`,
    transformOrigin: "center center",
    cursor: transform.scale > 1 ? "grab" : "default",
  };

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Panels */}
      <div
        className="flex-1 flex"
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      >
        {/* Left panel (pinned) */}
        <div className="flex-1 relative overflow-hidden bg-[var(--bg-primary)] flex items-center justify-center">
          <div className="absolute top-3 left-3 px-2.5 py-1 rounded text-[11px] font-semibold bg-[var(--accent)]/15 border border-[var(--accent)]/30 text-blue-300 z-10">
            ① Pinned
          </div>
          {transform.scale > 1 && (
            <div className="absolute top-3 left-1/2 -translate-x-1/2 bg-black/60 text-white/70 text-[10px] px-2 py-0.5 rounded z-10">
              {Math.round(transform.scale * 100)}%
            </div>
          )}
          {pinnedUrl && (
            <img
              src={pinnedUrl}
              alt={pinnedImage?.filename}
              className="max-w-full max-h-full object-contain"
              style={imgStyle}
              draggable={false}
            />
          )}
          <div className="absolute bottom-3 left-1/2 -translate-x-1/2 text-xl font-bold px-4 py-1.5 rounded-lg bg-[var(--accent)]/15 text-blue-300 border border-[var(--accent)]/25 opacity-60 pointer-events-none">
            1
          </div>
        </div>

        {/* Divider */}
        <div className="w-0.5 bg-white/10 flex-shrink-0" />

        {/* Right panel (cycling) */}
        <div className="flex-1 relative overflow-hidden bg-[var(--bg-primary)] flex items-center justify-center">
          <div className="absolute top-3 right-3 px-2.5 py-1 rounded text-[11px] font-semibold bg-purple-500/15 border border-purple-500/30 text-purple-300 z-10">
            ② Cycling ← →
          </div>
          {transform.scale > 1 && (
            <div className="absolute top-3 left-1/2 -translate-x-1/2 bg-black/60 text-white/70 text-[10px] px-2 py-0.5 rounded z-10">
              {Math.round(transform.scale * 100)}%
            </div>
          )}
          {cyclingUrl && (
            <img
              src={cyclingUrl}
              alt={cyclingImage?.filename}
              className="max-w-full max-h-full object-contain"
              style={imgStyle}
              draggable={false}
            />
          )}
          <div className="absolute bottom-3 left-1/2 -translate-x-1/2 text-xl font-bold px-4 py-1.5 rounded-lg bg-purple-500/15 text-purple-300 border border-purple-500/25 opacity-60 pointer-events-none">
            2
          </div>
          <div className="absolute bottom-3 right-3 text-[10px] text-[var(--text-secondary)] flex items-center gap-1">
            <span className="text-purple-400">◀</span>
            {cyclingIdx + 1} / {availableMembers.length}
            <span className="text-purple-400">▶</span>
          </div>
        </div>
      </div>

      {/* EXIF comparison strip */}
      <div className="flex border-t border-white/5 bg-[#0d0d0d]">
        <ExifPanel image={pinnedImage} />
        <div className="w-px bg-white/5" />
        <ExifPanel image={cyclingImage} />
      </div>

      {/* Group member strip */}
      <div className="flex gap-1 justify-center py-2 px-4 bg-[#111] border-t border-white/10">
        {availableMembers.map((id) => (
          <MemberThumb
            key={id}
            photoId={id}
            isPinned={id === comparisonPinnedId}
            isCycling={id === comparisonCyclingId}
          />
        ))}
      </div>
    </div>
  );
}

function ExifPanel({ image }: { image: ReturnType<typeof useProjectStore.getState>["images"][0] | null }) {
  if (!image) return <div className="flex-1 p-2" />;

  return (
    <div className="flex-1 flex gap-4 px-4 py-2 text-[11px] text-[var(--text-secondary)]">
      {image.aperture && (
        <span>
          f/<span className="text-[var(--text-primary)]">{image.aperture}</span>
        </span>
      )}
      {image.shutterSpeed && (
        <span className="text-[var(--text-primary)]">{image.shutterSpeed}</span>
      )}
      {image.iso && (
        <span>
          ISO <span className="text-[var(--text-primary)]">{image.iso}</span>
        </span>
      )}
      {image.focalLength && (
        <span className="text-[var(--text-primary)]">{image.focalLength}mm</span>
      )}
    </div>
  );
}

function MemberThumb({
  photoId,
  isPinned,
  isCycling,
}: {
  photoId: number;
  isPinned: boolean;
  isCycling: boolean;
}) {
  const [loaded, setLoaded] = useState(false);

  return (
    <div className="relative">
      <div
        className={`rounded overflow-hidden ${
          isPinned
            ? "ring-2 ring-[var(--accent)]"
            : isCycling
              ? "ring-2 ring-purple-500"
              : "brightness-75"
        }`}
        style={{ width: 50, height: 38 }}
      >
        <img
          src={thumbUrl(photoId)}
          alt=""
          className={`w-full h-full object-cover ${loaded ? "opacity-100" : "opacity-30"}`}
          loading="lazy"
          draggable={false}
          onLoad={(e) => {
            if (e.currentTarget.naturalWidth > 1) setLoaded(true);
          }}
        />
      </div>
      {(isPinned || isCycling) && (
        <div
          className={`absolute -bottom-1 left-1/2 -translate-x-1/2 w-1 h-1 rounded-full ${
            isPinned ? "bg-[var(--accent)]" : "bg-purple-500"
          }`}
        />
      )}
    </div>
  );
}
