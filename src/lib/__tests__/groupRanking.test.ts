import { describe, it, expect } from "vitest";
import { computeGroupRanks, rankColorClass } from "../groupRanking";

describe("computeGroupRanks", () => {
  it("emits no color for single-member groups", () => {
    const ranks = computeGroupRanks([{ id: 1, qualityScore: 80 }]);
    expect(ranks.get(1)).toEqual({ id: 1, rank: null, color: null });
  });

  it("emits no color when nothing has been analyzed", () => {
    const ranks = computeGroupRanks([
      { id: 1, qualityScore: null },
      { id: 2, qualityScore: null },
    ]);
    expect(ranks.get(1)?.color).toBeNull();
    expect(ranks.get(2)?.color).toBeNull();
  });

  it("best in a pair is green, worst is red", () => {
    const ranks = computeGroupRanks([
      { id: 1, qualityScore: 50 },
      { id: 2, qualityScore: 80 },
    ]);
    expect(ranks.get(2)?.color).toBe("green");
    expect(ranks.get(1)?.color).toBe("red");
    expect(ranks.get(2)?.rank).toBe(0);
  });

  it("middle member of a triple is white", () => {
    const ranks = computeGroupRanks([
      { id: 1, qualityScore: 30 },
      { id: 2, qualityScore: 60 },
      { id: 3, qualityScore: 90 },
    ]);
    expect(ranks.get(3)?.color).toBe("green");
    expect(ranks.get(2)?.color).toBe("white");
    expect(ranks.get(1)?.color).toBe("red");
  });

  it("unanalyzed members in a partly-analyzed group get null color", () => {
    const ranks = computeGroupRanks([
      { id: 1, qualityScore: 70 },
      { id: 2, qualityScore: null },
      { id: 3, qualityScore: 40 },
    ]);
    expect(ranks.get(1)?.color).toBe("green");
    expect(ranks.get(3)?.color).toBe("white");
    expect(ranks.get(2)?.color).toBeNull();
    expect(ranks.get(2)?.rank).toBeNull();
  });

  it("handles larger groups by rank percentile", () => {
    // 6 members: 2 green, 2 white, 2 red (rough thirds)
    const ranks = computeGroupRanks([
      { id: 1, qualityScore: 10 },
      { id: 2, qualityScore: 20 },
      { id: 3, qualityScore: 30 },
      { id: 4, qualityScore: 40 },
      { id: 5, qualityScore: 50 },
      { id: 6, qualityScore: 60 },
    ]);
    // Best (60, 50) → green; middle (40, 30) → white; worst (20, 10) → red.
    expect(ranks.get(6)?.color).toBe("green");
    expect(ranks.get(5)?.color).toBe("green");
    expect(ranks.get(4)?.color).toBe("white");
    expect(ranks.get(3)?.color).toBe("white");
    expect(ranks.get(2)?.color).toBe("red");
    expect(ranks.get(1)?.color).toBe("red");
  });
});

describe("rankColorClass", () => {
  it("maps colors to tailwind utility classes", () => {
    expect(rankColorClass("green")).toBe("bg-emerald-500");
    expect(rankColorClass("white")).toBe("bg-white");
    expect(rankColorClass("red")).toBe("bg-red-500");
    expect(rankColorClass(null)).toBeNull();
  });
});
