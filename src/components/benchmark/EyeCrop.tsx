import { useState, type SyntheticEvent } from "react";
import { imageUrl } from "../../hooks/useImageLoader";

interface Props {
  photoId: number;
  /// Normalized 0–1 eye coordinates from YuNet (per-face Face row).
  eyeXNormalized: number;
  eyeYNormalized: number;
  /// Normalized 0–1 face bbox width — drives the crop side length
  /// (matches Rust's `eye_crop_pixels`: 15% of face width, square).
  bboxWNormalized: number;
  sizePx: number;
  label?: string;
}

const EYE_CROP_RATIO = 0.15;

/// A tiny thumbnail that renders the **exact pixel patch** the eye
/// classifier sees. Crop is centered on YuNet's eye landmark and is a
/// square of side `0.15 × face_bbox_width × image_natural_width`,
/// mirroring `src-tauri/src/ai/eye.rs::eye_crop_pixels`. If the
/// classifier says "closed" on a crop that is clearly an eyebrow,
/// you'll see it here at a glance — the bug is in the landmark, not
/// in the classifier.
///
/// CSS strategy borrowed from `FaceThumb.tsx`: load the full preview
/// at natural size, then translate + scale so the crop region fills
/// the fixed-size container. No canvas, no extra fetch — the preview
/// is already cached by `LoupeView`.
export function EyeCrop({
  photoId,
  eyeXNormalized,
  eyeYNormalized,
  bboxWNormalized,
  sizePx,
  label,
}: Props) {
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);

  const onLoad = (e: SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    if (img.naturalWidth > 0 && img.naturalHeight > 0) {
      setNatural({ w: img.naturalWidth, h: img.naturalHeight });
    }
  };

  let transform = "";
  if (natural) {
    const cropPx = Math.max(1, EYE_CROP_RATIO * bboxWNormalized * natural.w);
    const scale = sizePx / cropPx;
    const cx = eyeXNormalized * natural.w;
    const cy = eyeYNormalized * natural.h;
    const tx = sizePx / 2 - cx * scale;
    const ty = sizePx / 2 - cy * scale;
    transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
  }

  return (
    <div
      className="relative overflow-hidden rounded-xs flex-shrink-0"
      style={{
        width: sizePx,
        height: sizePx,
        background: "var(--color-bg2)",
        border: "1px solid var(--color-border)",
      }}
      title={label}
    >
      <img
        src={imageUrl(photoId)}
        alt={label ?? ""}
        aria-hidden={label ? undefined : true}
        onLoad={onLoad}
        draggable={false}
        className="absolute top-0 left-0 max-w-none origin-top-left"
        style={{
          transform,
          visibility: natural ? "visible" : "hidden",
        }}
      />
    </div>
  );
}
