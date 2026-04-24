import { describe, it, expect } from "vitest";
import { createBracket, applyDecision, currentPair } from "../bracket";

describe("createBracket", () => {
  it("returns a complete bracket for an empty group", () => {
    const b = createBracket(42, []);
    expect(b.groupId).toBe(42);
    expect(b.isComplete).toBe(true);
    expect(b.rounds).toHaveLength(0);
    expect(currentPair(b)).toBeNull();
  });

  it("is complete immediately for a single-member group (nothing to compare)", () => {
    const b = createBracket(1, [{ id: 7, qualityScore: 80 }]);
    expect(b.isComplete).toBe(true);
    expect(currentPair(b)).toBeNull();
  });

  it("seeds Round 1 with a single pair for a 2-member group", () => {
    const b = createBracket(1, [
      { id: 10, qualityScore: 50 },
      { id: 20, qualityScore: 80 },
    ]);
    expect(b.isComplete).toBe(false);
    expect(b.rounds).toHaveLength(1);
    expect(b.rounds[0].pairs).toHaveLength(1);
    expect(b.currentRound).toBe(0);
    expect(b.currentPairIndex).toBe(0);
    const pair = currentPair(b);
    expect(pair).not.toBeNull();
    // Higher quality seeded as left.
    expect(pair!.left).toBe(20);
    expect(pair!.right).toBe(10);
  });

  it("odd-sized group produces a bye as the last pair", () => {
    const b = createBracket(1, [
      { id: 1, qualityScore: 90 },
      { id: 2, qualityScore: 80 },
      { id: 3, qualityScore: 70 },
    ]);
    expect(b.rounds[0].pairs).toHaveLength(2);
    expect(b.rounds[0].pairs[1]).toEqual({ left: 3, right: null });
    expect(b.rounds[0].results[1]).toBe("bye");
    // currentPair should skip straight to the decidable pair, not the bye.
    expect(currentPair(b)).toEqual({ left: 1, right: 2 });
  });

  it("treats null quality scores as lowest and breaks ties by id", () => {
    const b = createBracket(1, [
      { id: 3, qualityScore: null },
      { id: 1, qualityScore: 50 },
      { id: 2, qualityScore: 50 },
      { id: 4, qualityScore: null },
    ]);
    // Expected seed order: 1,2 (tie, id asc), then 3,4 (nulls, id asc).
    expect(b.rounds[0].pairs[0]).toEqual({ left: 1, right: 2 });
    expect(b.rounds[0].pairs[1]).toEqual({ left: 3, right: 4 });
  });
});

describe("applyDecision", () => {
  it("L promotes the left photo, R is eliminated, bracket completes for a 2-member group", () => {
    const b = createBracket(1, [
      { id: 10, qualityScore: 80 },
      { id: 20, qualityScore: 50 },
    ]);
    const after = applyDecision(b, "L");
    expect(after.lastPromoted).toEqual([10]);
    expect(after.isComplete).toBe(true);
  });

  it("R promotes the right photo", () => {
    const b = createBracket(1, [
      { id: 10, qualityScore: 80 },
      { id: 20, qualityScore: 50 },
    ]);
    const after = applyDecision(b, "R");
    expect(after.lastPromoted).toEqual([20]);
    expect(after.isComplete).toBe(true);
  });

  it("Both promotes both members", () => {
    const b = createBracket(1, [
      { id: 10, qualityScore: 80 },
      { id: 20, qualityScore: 50 },
    ]);
    const after = applyDecision(b, "both");
    // Both survive Round 1 with 2 members → Round 2 would be {10,20} again,
    // which is the same survivor set as Round 1 (degenerate) → complete.
    expect(after.lastPromoted.sort()).toEqual([10, 20]);
    expect(after.isComplete).toBe(true);
  });

  it("spawns Round 2 with survivors when Round 1 has multiple pairs", () => {
    const b = createBracket(1, [
      { id: 1, qualityScore: 100 },
      { id: 2, qualityScore: 90 },
      { id: 3, qualityScore: 80 },
      { id: 4, qualityScore: 70 },
    ]);
    // Round 1 pairs: (1,2), (3,4).
    let s = applyDecision(b, "L"); // 1 wins, 2 eliminated
    expect(s.isComplete).toBe(false);
    expect(s.currentRound).toBe(0);
    expect(s.currentPairIndex).toBe(1);
    expect(currentPair(s)).toEqual({ left: 3, right: 4 });
    s = applyDecision(s, "R"); // 4 wins, 3 eliminated
    // Survivors: [1, 4]. Round 2 pairs (1,4).
    expect(s.isComplete).toBe(false);
    expect(s.currentRound).toBe(1);
    expect(s.rounds).toHaveLength(2);
    expect(currentPair(s)).toEqual({ left: 1, right: 4 });
    // Finish Round 2: pick L.
    s = applyDecision(s, "L");
    expect(s.isComplete).toBe(true);
    expect(s.lastPromoted).toEqual([1]);
  });

  it("byes survive to the next round without user input", () => {
    const b = createBracket(1, [
      { id: 1, qualityScore: 90 },
      { id: 2, qualityScore: 80 },
      { id: 3, qualityScore: 70 },
    ]);
    // Round 1 pairs: (1,2), (3 bye). currentPair should skip the bye.
    expect(currentPair(b)).toEqual({ left: 1, right: 2 });
    const s = applyDecision(b, "L");
    // Survivors: [1, 3]. Round 2 pairs (1,3).
    expect(s.currentRound).toBe(1);
    expect(currentPair(s)).toEqual({ left: 1, right: 3 });
  });

  it("completes via degenerate guard when no round eliminates anyone", () => {
    const b = createBracket(1, [
      { id: 1, qualityScore: 100 },
      { id: 2, qualityScore: 90 },
      { id: 3, qualityScore: 80 },
      { id: 4, qualityScore: 70 },
    ]);
    let s = applyDecision(b, "both"); // pair (1,2) both survive
    s = applyDecision(s, "both");     // pair (3,4) both survive
    // Round 1 survivors = {1,2,3,4} = original seed → degenerate → complete.
    expect(s.isComplete).toBe(true);
  });

  it("no-ops when called on a complete bracket", () => {
    const b = createBracket(1, [{ id: 1, qualityScore: 80 }]);
    const s = applyDecision(b, "L");
    expect(s).toBe(b);
  });

  it("handles a 5-member group with a bye in every round", () => {
    const b = createBracket(1, [
      { id: 1, qualityScore: 100 },
      { id: 2, qualityScore: 90 },
      { id: 3, qualityScore: 80 },
      { id: 4, qualityScore: 70 },
      { id: 5, qualityScore: 60 },
    ]);
    // Round 1: (1,2), (3,4), 5 bye.
    expect(b.rounds[0].pairs).toHaveLength(3);
    expect(b.rounds[0].pairs[2]).toEqual({ left: 5, right: null });
    let s = applyDecision(b, "L"); // 1 wins
    s = applyDecision(s, "L");     // 3 wins
    // Survivors: [1, 3, 5]. Round 2: (1,3), 5 bye.
    expect(s.currentRound).toBe(1);
    expect(s.rounds[1].pairs).toHaveLength(2);
    expect(currentPair(s)).toEqual({ left: 1, right: 3 });
    s = applyDecision(s, "L"); // 1 wins again
    // Survivors: [1, 5]. Round 3: (1, 5).
    expect(s.currentRound).toBe(2);
    expect(currentPair(s)).toEqual({ left: 1, right: 5 });
    s = applyDecision(s, "L");
    expect(s.isComplete).toBe(true);
  });
});
