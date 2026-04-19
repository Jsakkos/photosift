/// Pure helpers for LoupeView zoom math. Extracted so the ratio and
/// clamp logic can be unit-tested without a DOM — LoupeView itself
/// supplies the measured `getBoundingClientRect` + `naturalWidth` and
/// passes them in.

/// Returns the CSS transform scale that maps the image's rendered
/// pixels to its natural pixels (true 1:1 / "100% crop"). Assumes the
/// image is displayed with `object-contain` semantics inside the box —
/// whichever axis is tighter determines the rendered size.
export function computeNativeScale(
  naturalW: number,
  naturalH: number,
  boxW: number,
  boxH: number,
): number {
  if (naturalW <= 0 || naturalH <= 0 || boxW <= 0 || boxH <= 0) return 1;
  const imgAspect = naturalW / naturalH;
  const boxAspect = boxW / boxH;
  const renderedW = imgAspect > boxAspect ? boxW : boxH * imgAspect;
  if (renderedW <= 0) return 1;
  return naturalW / renderedW;
}

/// Scroll-wheel zoom step with `[1, 2*native]` clamping. `delta` is the
/// raw multiplier (0.9 zoom out, 1.1 zoom in per the ComparisonView
/// convention).
export function clampZoomScale(
  current: number,
  delta: number,
  nativeScale: number,
): number {
  const max = Math.max(nativeScale * 2, 1);
  return Math.max(1, Math.min(max, current * delta));
}
