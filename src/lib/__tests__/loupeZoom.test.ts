import { describe, it, expect } from "vitest";
import { computeNativeScale, clampZoomScale } from "../loupeZoom";

describe("computeNativeScale", () => {
  it("D750 NEF (6016x4016) letterboxed in a 2560x1440 box fits height", () => {
    // imageAspect 1.497 < boxAspect 1.778 → fits height.
    // renderedW = 1440 * 6016/4016 ≈ 2157.4
    // nativeScale = 6016 / 2157.4 ≈ 2.789
    const s = computeNativeScale(6016, 4016, 2560, 1440);
    expect(s).toBeCloseTo(2.789, 2);
  });

  it("returns 1.0 when image matches container exactly", () => {
    expect(computeNativeScale(1000, 1000, 1000, 1000)).toBeCloseTo(1.0, 5);
  });

  it("ultrawide image narrower-than-aspect container fits width", () => {
    // 4000x1000 (aspect 4) in 2000x2000 (aspect 1). Fits width.
    // renderedW = 2000 → nativeScale = 2.
    expect(computeNativeScale(4000, 1000, 2000, 2000)).toBeCloseTo(2.0, 5);
  });

  it("matching aspect ratio: scale is the width ratio", () => {
    // 4000x3000 (4:3) in 2000x1500 (4:3). Fits width.
    expect(computeNativeScale(4000, 3000, 2000, 1500)).toBeCloseTo(2.0, 5);
  });

  it("returns 1.0 on degenerate dimensions", () => {
    // Defensive: a zero-width rect (loaded before layout) should
    // degrade to 1 rather than Infinity or NaN.
    expect(computeNativeScale(6016, 4016, 0, 1440)).toBe(1);
    expect(computeNativeScale(0, 4016, 2560, 1440)).toBe(1);
  });
});

describe("clampZoomScale", () => {
  const native = 2.789;

  it("applies delta multiplier", () => {
    expect(clampZoomScale(native, 1.1, native)).toBeCloseTo(native * 1.1, 3);
    expect(clampZoomScale(native, 0.9, native)).toBeCloseTo(native * 0.9, 3);
  });

  it("clamps to minimum of 1.0 (never below fit)", () => {
    expect(clampZoomScale(1.0, 0.5, native)).toBe(1);
    expect(clampZoomScale(0.3, 1.1, native)).toBeGreaterThanOrEqual(1);
  });

  it("clamps to 2x native on the high end", () => {
    expect(clampZoomScale(native * 1.9, 1.2, native)).toBeCloseTo(native * 2, 3);
    expect(clampZoomScale(10, 1.1, native)).toBeCloseTo(native * 2, 3);
  });

  it("allows intermediate values between 1 and 2x native", () => {
    const result = clampZoomScale(1.5, 1.1, native);
    expect(result).toBeCloseTo(1.65, 3);
    expect(result).toBeLessThan(native * 2);
    expect(result).toBeGreaterThan(1);
  });
});
