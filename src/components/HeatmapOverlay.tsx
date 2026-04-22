import { useEffect, useRef, useState } from "react";
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
  // Aspect ratio of the underlying image, so the canvas letterboxes
  // the same way LoupeView's `<img object-contain>` does. Without
  // this the heatmap stretches across the full container and its
  // tiles no longer line up with image features.
  const [aspect, setAspect] = useState<number | null>(null);

  useEffect(() => {
    if (!heatmapOn) return;
    // The browser has this image cached from LoupeView, so this is
    // essentially a free naturalWidth/Height read.
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
  }, [heatmapOn, photoId, getData, aspect]);

  if (!heatmapOn) return null;

  return (
    <div className="absolute inset-0 flex items-center justify-center pointer-events-none z-[5]">
      <canvas
        ref={canvasRef}
        width={GRID_COLS}
        height={GRID_ROWS}
        aria-hidden="true"
        className="opacity-35"
        style={{
          maxWidth: "100%",
          maxHeight: "100%",
          aspectRatio: aspect ?? undefined,
          // Fall back to filling the box until we know the aspect —
          // still better than the old always-stretched behavior for
          // 3:2 shots which are the D750 default.
          width: aspect ? undefined : "100%",
          height: aspect ? undefined : "100%",
        }}
      />
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
