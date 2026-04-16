import { useRef, useState, useCallback } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useImageLoader } from "../hooks/useImageLoader";
import { FlagFlash } from "./FlagFlash";

export function LoupeView() {
  const { displayItems, currentIndex, isZoomed, toggleZoom, currentView } = useProjectStore();
  const currentImage = displayItems[currentIndex]?.image ?? null;
  const { displayUrl } = useImageLoader(currentImage?.id ?? null);

  const containerRef = useRef<HTMLDivElement>(null);
  const [zoomOrigin, setZoomOrigin] = useState({ x: 50, y: 50 });
  const [panOffset, setPanOffset] = useState({ x: 0, y: 0 });
  const isDragging = useRef(false);
  const dragStart = useRef({ x: 0, y: 0 });

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!isZoomed) return;
      isDragging.current = true;
      dragStart.current = { x: e.clientX - panOffset.x, y: e.clientY - panOffset.y };
    },
    [isZoomed, panOffset],
  );

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging.current) return;
    setPanOffset({
      x: e.clientX - dragStart.current.x,
      y: e.clientY - dragStart.current.y,
    });
  }, []);

  const handleMouseUp = useCallback(() => {
    isDragging.current = false;
  }, []);

  const handleDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      if (!containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const x = ((e.clientX - rect.left) / rect.width) * 100;
      const y = ((e.clientY - rect.top) / rect.height) * 100;
      setZoomOrigin({ x, y });
      setPanOffset({ x: 0, y: 0 });
      toggleZoom();
    },
    [toggleZoom],
  );

  if (!currentImage || !displayUrl) {
    return (
      <div className="absolute inset-0 flex items-center justify-center bg-[var(--bg-primary)]">
        <p className="text-[var(--text-secondary)]">No image selected</p>
      </div>
    );
  }

  const imgStyle: React.CSSProperties = isZoomed
    ? {
        transform: `scale(3) translate(${panOffset.x / 3}px, ${panOffset.y / 3}px)`,
        transformOrigin: `${zoomOrigin.x}% ${zoomOrigin.y}%`,
        cursor: "grab",
      }
    : { transform: "scale(1)", transformOrigin: "center center", cursor: "default" };

  return (
    <div
      ref={containerRef}
      className="absolute inset-0 overflow-hidden bg-[var(--bg-primary)]"
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onDoubleClick={handleDoubleClick}
    >
      <img
        src={displayUrl}
        alt={currentImage.filename}
        className="w-full h-full object-contain transition-transform duration-100"
        style={imgStyle}
        draggable={false}
      />
      <FlagFlash />
      {currentView === "triage" && displayItems[currentIndex]?.isGroupCover && (
        <div className="absolute top-4 right-4 px-3 py-1.5 rounded-md bg-[var(--accent)]/15 border border-[var(--accent)]/30 text-[var(--accent)] text-xs font-medium pointer-events-none flex items-center gap-2">
          <span className="flex flex-col gap-0.5">
            <span className="block w-2.5 h-0.5 bg-current rounded-sm" />
            <span className="block w-2.5 h-0.5 bg-current rounded-sm" />
            <span className="block w-2.5 h-0.5 bg-current rounded-sm" />
          </span>
          Group · {displayItems[currentIndex]?.groupMemberCount} photos
        </div>
      )}
      {currentImage.flag !== "unreviewed" && (
        <div
          className={`absolute top-4 left-4 px-3 py-1 rounded-full text-xs font-medium uppercase tracking-wide pointer-events-none ${
            currentImage.flag === "pick"
              ? "bg-green-500/80 text-white"
              : "bg-red-500/80 text-white"
          }`}
        >
          {currentImage.flag}
        </div>
      )}
    </div>
  );
}
