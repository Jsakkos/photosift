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

  const onLoad = (e: SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    if (img.naturalWidth > 0 && img.naturalHeight > 0) {
      setNatural({ w: img.naturalWidth, h: img.naturalHeight });
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
      className="relative overflow-hidden rounded bg-black/40 flex-shrink-0"
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
          visibility: natural ? "visible" : "hidden",
        }}
      />
    </div>
  );
}
