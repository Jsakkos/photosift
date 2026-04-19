import { useEffect, useRef } from "react";
import { useProjectStore } from "../stores/projectStore";

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

  useEffect(() => {
    if (!heatmapOn) return;
    const data = getData(photoId);
    if (data) {
      draw(canvasRef.current, data);
      return;
    }
    // Data not cached yet — subscribe to the cache map for this photo,
    // drawing once the backend returns.
    const unsub = useProjectStore.subscribe((state, prev) => {
      if (state.heatmapCache === prev.heatmapCache) return;
      const grid = state.heatmapCache.get(photoId);
      if (grid) {
        draw(canvasRef.current, grid);
        unsub();
      }
    });
    return unsub;
  }, [heatmapOn, photoId, getData]);

  if (!heatmapOn) return null;

  return (
    <canvas
      ref={canvasRef}
      width={GRID_COLS}
      height={GRID_ROWS}
      aria-hidden="true"
      className="absolute inset-0 w-full h-full pointer-events-none z-[5] opacity-35"
    />
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
