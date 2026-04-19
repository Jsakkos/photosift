import { describe, it, expect, beforeEach } from "vitest";
import {
  useAiStore,
  sharpnessBadgeScore,
  sharpnessBandColor,
} from "../aiStore";
import type { SharpnessPercentiles } from "../../types";

describe("aiStore", () => {
  beforeEach(() => {
    useAiStore.setState({ provider: "disabled", analyzed: 0, failed: 0, total: 0 });
  });

  it("patches progress counters from ai-progress event", () => {
    useAiStore.getState().handleProgress({
      photoId: 5, ok: true, done: 3, total: 10, failed: 0,
    });
    const s = useAiStore.getState();
    expect(s.analyzed).toBe(3);
    expect(s.total).toBe(10);
    expect(s.failed).toBe(0);
  });

  it("tracks failures separately", () => {
    useAiStore.getState().handleProgress({ photoId: 5, ok: false, done: 4, total: 10, failed: 2 });
    const s = useAiStore.getState();
    expect(s.failed).toBe(2);
    expect(s.analyzed).toBe(2); // done=4 minus failed=2
    expect(s.total).toBe(10);
  });

  it("resets counters on reset()", () => {
    useAiStore.getState().handleProgress({ photoId: 5, ok: true, done: 3, total: 10, failed: 0 });
    useAiStore.getState().reset();
    const s = useAiStore.getState();
    expect(s.analyzed).toBe(0);
    expect(s.total).toBe(0);
    expect(s.failed).toBe(0);
  });

  it("setProvider changes the provider status", () => {
    useAiStore.getState().setProvider("cuda");
    expect(useAiStore.getState().provider).toBe("cuda");
    useAiStore.getState().setProvider("disabled");
    expect(useAiStore.getState().provider).toBe("disabled");
  });
});

describe("sharpnessBadgeScore", () => {
  const p: SharpnessPercentiles = {
    p10: 20,
    p30: 40,
    p50: 50,
    p70: 60,
    p90: 80,
    analyzedCount: 100,
    analyzedMaxTs: "2026-04-18T00:00:00",
  };

  it("returns 10 at or above p90", () => {
    expect(sharpnessBadgeScore(80, p)).toBe(10);
    expect(sharpnessBadgeScore(95, p)).toBe(10);
  });

  it("maps into the 8-9 band between p70 and p90", () => {
    const s = sharpnessBadgeScore(70, p);
    expect(s).toBeGreaterThanOrEqual(8);
    expect(s).toBeLessThanOrEqual(9);
  });

  it("returns 1 below p10", () => {
    expect(sharpnessBadgeScore(5, p)).toBe(1);
  });

  it("falls back to raw/10 when percentiles are unavailable", () => {
    expect(sharpnessBadgeScore(73, null)).toBe(7);
    expect(sharpnessBadgeScore(0, null)).toBe(1);
    expect(sharpnessBadgeScore(99, null)).toBe(10);
  });

  it("returns 1 for null raw score (unanalyzed)", () => {
    expect(sharpnessBadgeScore(null, p)).toBe(1);
    expect(sharpnessBadgeScore(undefined, p)).toBe(1);
  });

  it("band color partitions 1-10 into red/yellow/green", () => {
    expect(sharpnessBandColor(1)).toBe("red");
    expect(sharpnessBandColor(3)).toBe("red");
    expect(sharpnessBandColor(4)).toBe("yellow");
    expect(sharpnessBandColor(7)).toBe("yellow");
    expect(sharpnessBandColor(8)).toBe("green");
    expect(sharpnessBandColor(10)).toBe("green");
  });
});
