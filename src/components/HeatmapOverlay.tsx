import { useEffect, useRef } from "react";
import { useProjectStore } from "../stores/projectStore";

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
      width={32}
      height={32}
      aria-hidden="true"
      className="absolute inset-0 w-full h-full pointer-events-none z-[5] opacity-35"
      style={{ imageRendering: "pixelated" }}
    />
  );
}

function draw(canvas: HTMLCanvasElement | null, data: number[]) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const img = ctx.createImageData(32, 32);
  for (let i = 0; i < 1024; i++) {
    const v = Math.max(0, Math.min(100, data[i] ?? 0)) / 100;
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
