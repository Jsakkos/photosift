import { useState, type SyntheticEvent } from "react";
import { imageUrl } from "../hooks/useImageLoader";
import type { Face } from "../types";

/// Uniform-scale face crop. The naive width/height percentage approach
/// stretches non-uniformly whenever the source image aspect differs from
/// the face-bbox aspect — which is almost always (bounding boxes are
/// roughly square, D750 frames are 3:2). We read naturalWidth/Height on
/// load and emit a CSS transform that scales uniformly (cover semantics)
/// and translates the face center into the container center.
///
/// Uses the full-resolution embedded preview (`imageUrl`) rather than
/// the 512px thumbnail — a face is often ~15% of the frame, so cropping
/// from a thumb requires a ~2x CSS upscale that looks blurry. The
/// full-res preview is already in the LoupeView's cache by the time
/// the panel renders, so there's no extra network hit.
///
/// Auto-exposure: once the image loads, sample the face region into a
/// tiny offscreen canvas and compute mean luminance + p5/p95. We then
/// apply a `brightness()/contrast()` CSS filter to lift underexposed
/// crops into a readable range. Clamps the brightness floor at 1.0 so
/// already-exposed faces are never darkened. Rust-free; tauri:// assets
/// are same-origin so getImageData works without CORS headers.
function autoExposureFilter(
  img: HTMLImageElement,
  face: Face,
  natural: { w: number; h: number },
): string | undefined {
  const SAMPLE = 32;
  const canvas = document.createElement("canvas");
  canvas.width = SAMPLE;
  canvas.height = SAMPLE;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return undefined;

  const sx = Math.max(0, face.bboxX * natural.w);
  const sy = Math.max(0, face.bboxY * natural.h);
  const sw = Math.max(1, face.bboxW * natural.w);
  const sh = Math.max(1, face.bboxH * natural.h);

  try {
    ctx.drawImage(img, sx, sy, sw, sh, 0, 0, SAMPLE, SAMPLE);
    const { data } = ctx.getImageData(0, 0, SAMPLE, SAMPLE);
    const lums = new Float32Array(SAMPLE * SAMPLE);
    let sum = 0;
    for (let i = 0, j = 0; i < data.length; i += 4, j++) {
      const L = 0.2126 * data[i] + 0.7152 * data[i + 1] + 0.0722 * data[i + 2];
      lums[j] = L;
      sum += L;
    }
    const mean = sum / lums.length;
    // Float32Array.sort() is numeric by default and sorts in place; no
    // comparator needed and no JS-heap copy.
    lums.sort();
    const p5 = lums[Math.floor(lums.length * 0.05)];
    const p95 = lums[Math.floor(lums.length * 0.95)];

    const TARGET_MEAN = 130;
    const brightness = Math.max(
      1.0,
      Math.min(2.5, TARGET_MEAN / Math.max(mean, 20)),
    );
    const range = p95 - p5;
    const contrast =
      range < 120 ? Math.max(1.0, Math.min(1.6, 150 / Math.max(range, 30))) : 1.0;

    if (brightness < 1.02 && contrast < 1.02) return undefined;
    return `brightness(${brightness.toFixed(2)}) contrast(${contrast.toFixed(2)})`;
  } catch {
    return undefined;
  }
}

export function FaceThumb({
  face,
  photoId,
  sizePx,
}: {
  face: Face;
  photoId: number;
  sizePx: number;
}) {
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [filter, setFilter] = useState<string | undefined>(undefined);

  const onLoad = (e: SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    if (img.naturalWidth > 0 && img.naturalHeight > 0) {
      const nat = { w: img.naturalWidth, h: img.naturalHeight };
      setNatural(nat);
      setFilter(autoExposureFilter(img, face, nat));
    }
  };

  let transform = "";
  if (natural) {
    const facePxW = face.bboxW * natural.w;
    const facePxH = face.bboxH * natural.h;
    // `cover`: max scale so the face fills the container; excess on the
    // longer axis gets clipped rather than the image being squashed.
    const scale = Math.max(sizePx / facePxW, sizePx / facePxH);
    const faceCenterX = (face.bboxX + face.bboxW / 2) * natural.w;
    const faceCenterY = (face.bboxY + face.bboxH / 2) * natural.h;
    const tx = sizePx / 2 - faceCenterX * scale;
    const ty = sizePx / 2 - faceCenterY * scale;
    transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
  }

  return (
    <div
      className="relative overflow-hidden rounded-xs bg-black/40 flex-shrink-0"
      style={{ width: sizePx, height: sizePx }}
    >
      <img
        src={imageUrl(photoId)}
        alt=""
        aria-hidden="true"
        onLoad={onLoad}
        className="absolute top-0 left-0 max-w-none origin-top-left"
        style={{
          transform,
          filter,
          visibility: natural ? "visible" : "hidden",
        }}
      />
    </div>
  );
}
