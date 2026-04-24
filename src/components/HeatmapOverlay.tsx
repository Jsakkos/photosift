import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useProjectStore } from "../stores/projectStore";
import { imageUrl } from "../hooks/useImageLoader";

// Must match the tile grid emitted by the Rust `get_heatmap` command
// (see `src-tauri/src/commands/ai.rs`). Fine tiling + bilinear CSS
// scaling reads as a continuous falloff instead of the 32×32 "pixel
// art" that came before.
const GRID_COLS = 48;
const GRID_ROWS = 32;

interface Props {
  photoId: number;
}

export function HeatmapOverlay({ photoId }: Props) {
  const heatmapOn = useProjectStore((s) => s.heatmapOn);
  const getData = useProjectStore((s) => s.getHeatmapData);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [aspect, setAspect] = useState<number | null>(null);
  // Letterboxed rect matching where LoupeView's `<img object-contain>`
  // actually draws. CSS alone can't express this: `aspect-ratio` +
  // `max-*` on a wrapper lets width or height dominate depending on the
  // container's own aspect, so the wrapper ends up at container size
  // rather than the image's displayed size. A ResizeObserver measures
  // the container and we compute the contain-fit rect directly.
  const [rect, setRect] = useState<{ w: number; h: number } | null>(null);

  useEffect(() => {
    if (!heatmapOn) return;
    const img = new Image();
    img.onload = () => {
      if (img.naturalWidth > 0 && img.naturalHeight > 0) {
        setAspect(img.naturalWidth / img.naturalHeight);
      }
    };
    img.src = imageUrl(photoId);
    return () => {
      img.onload = null;
    };
  }, [heatmapOn, photoId]);

  useLayoutEffect(() => {
    if (!heatmapOn || !aspect) return;
    const el = containerRef.current;
    if (!el) return;
    const compute = () => {
      const { width, height } = el.getBoundingClientRect();
      if (width === 0 || height === 0) return;
      const containerAspect = width / height;
      if (containerAspect > aspect) {
        // Container is wider than the image; letterbox horizontally.
        setRect({ w: Math.round(height * aspect), h: Math.round(height) });
      } else {
        // Container is taller (or equal); letterbox vertically.
        setRect({ w: Math.round(width), h: Math.round(width / aspect) });
      }
    };
    compute();
    const ro = new ResizeObserver(compute);
    ro.observe(el);
    return () => ro.disconnect();
  }, [heatmapOn, aspect]);

  useEffect(() => {
    if (!heatmapOn) return;
    const data = getData(photoId);
    if (data) {
      draw(canvasRef.current, data);
      return;
    }
    const unsub = useProjectStore.subscribe((state, prev) => {
      if (state.heatmapCache === prev.heatmapCache) return;
      const grid = state.heatmapCache.get(photoId);
      if (grid) {
        draw(canvasRef.current, grid);
        unsub();
      }
    });
    return unsub;
  }, [heatmapOn, photoId, getData, rect]);

  if (!heatmapOn) return null;

  return (
    <div
      ref={containerRef}
      className="absolute inset-0 flex items-center justify-center pointer-events-none z-[5]"
    >
      {rect && (
        <canvas
          ref={canvasRef}
          width={GRID_COLS}
          height={GRID_ROWS}
          aria-hidden="true"
          className="opacity-35"
          style={{ width: rect.w, height: rect.h, display: "block" }}
        />
      )}
    </div>
  );
}

function draw(canvas: HTMLCanvasElement | null, data: number[]) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const n = GRID_COLS * GRID_ROWS;
  const img = ctx.createImageData(GRID_COLS, GRID_ROWS);
  for (let i = 0; i < n; i++) {
    const raw = Math.max(0, Math.min(100, data[i] ?? 0)) / 100;
    // Gamma-lift midtones: sqrt moves the center of the palette from
    // ~0.5 toward ~0.7, so partially-focused regions read yellow/green
    // instead of being crushed into the red floor by the p5/p95
    // percentile squash upstream.
    const v = Math.sqrt(raw);
    // Red → Yellow → Green gradient.
    const r = Math.round(255 * (1 - v));
    const g = Math.round(255 * v);
    img.data[i * 4] = r;
    img.data[i * 4 + 1] = g;
    img.data[i * 4 + 2] = 0;
    img.data[i * 4 + 3] = 180;
  }
  ctx.putImageData(img, 0, 0);
}
