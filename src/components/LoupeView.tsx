import { useRef, useState, useCallback } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useImageLoader } from "../hooks/useImageLoader";

export function LoupeView() {
  const { images, currentIndex, isZoomed, toggleZoom } = useProjectStore();
  const currentImage = images[currentIndex] ?? null;
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
      <div className="flex-1 flex items-center justify-center bg-[var(--bg-primary)]">
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
      className="flex-1 relative overflow-hidden bg-[var(--bg-primary)]"
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
    </div>
  );
}
