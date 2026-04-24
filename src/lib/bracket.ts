/// Tournament-bracket engine for PhotoSift's Select-view 2-up flow.
///
/// A bracket seeds Round 1 with a group's members sorted by qualityScore,
/// then asks the user for an L/R/Both decision on each pair. Winners
/// advance to the next round. Byes auto-survive. The bracket completes
/// when ≤1 member remains or a round produces no eliminations.
///
/// The engine is pure: callers apply the returned promoted-id list to
/// their own rating store. No React, no Zustand, no IPC.

export type Decision = "L" | "R" | "both";

export interface BracketMember {
  id: number;
  qualityScore: number | null | undefined;
}

export interface Pair {
  left: number;
  right: number | null;   // null when this slot is a bye
}

export interface PairingRound {
  pairs: Pair[];
  results: Array<Decision | "bye" | null>;
}

export interface BracketState {
  groupId: number;
  /// Original quality-sorted member ids. Stable for the lifetime of the
  /// bracket; used to re-pair survivors in each subsequent round.
  seedOrder: number[];
  rounds: PairingRound[];
  currentRound: number;
  currentPairIndex: number;
  isComplete: boolean;
  lastPromoted: number[];
}

function sortByQuality(members: BracketMember[]): number[] {
  return [...members]
    .sort((a, b) => {
      const aq = typeof a.qualityScore === "number" ? a.qualityScore : -Infinity;
      const bq = typeof b.qualityScore === "number" ? b.qualityScore : -Infinity;
      if (bq !== aq) return bq - aq;
      return a.id - b.id;
    })
    .map((m) => m.id);
}

function buildRoundFromIds(ids: number[]): PairingRound {
  const pairs: Pair[] = [];
  for (let i = 0; i < ids.length; i += 2) {
    const left = ids[i];
    const right = i + 1 < ids.length ? ids[i + 1] : null;
    pairs.push({ left, right });
  }
  return {
    pairs,
    results: pairs.map((p) => (p.right === null ? "bye" : null)),
  };
}

function firstUndecidedPairIndex(round: PairingRound): number {
  return round.results.findIndex((r) => r === null);
}

export function createBracket(groupId: number, members: BracketMember[]): BracketState {
  const ids = sortByQuality(members);
  if (ids.length < 2) {
    return {
      groupId,
      seedOrder: ids,
      rounds: [],
      currentRound: 0,
      currentPairIndex: 0,
      isComplete: true,
      lastPromoted: [],
    };
  }
  const round = buildRoundFromIds(ids);
  const idx = firstUndecidedPairIndex(round);
  return {
    groupId,
    seedOrder: ids,
    rounds: [round],
    currentRound: 0,
    currentPairIndex: idx === -1 ? round.pairs.length : idx,
    isComplete: false,
    lastPromoted: [],
  };
}

export function currentPair(state: BracketState): Pair | null {
  if (state.isComplete) return null;
  const round = state.rounds[state.currentRound];
  if (!round) return null;
  const pair = round.pairs[state.currentPairIndex];
  return pair ?? null;
}

function survivorsOfRound(round: PairingRound): number[] {
  const out: number[] = [];
  round.pairs.forEach((pair, i) => {
    const r = round.results[i];
    if (r === "L") out.push(pair.left);
    else if (r === "R" && pair.right !== null) out.push(pair.right);
    else if (r === "both") {
      out.push(pair.left);
      if (pair.right !== null) out.push(pair.right);
    } else if (r === "bye") {
      out.push(pair.left);
    }
  });
  return out;
}

function sameSet(a: number[], b: number[]): boolean {
  if (a.length !== b.length) return false;
  const sa = new Set(a);
  for (const x of b) if (!sa.has(x)) return false;
  return true;
}

export function applyDecision(state: BracketState, decision: Decision): BracketState {
  if (state.isComplete) return state;
  const round = state.rounds[state.currentRound];
  if (!round) return state;
  const pair = round.pairs[state.currentPairIndex];
  if (!pair || pair.right === null) return state;

  const newRounds = state.rounds.map((r, i) =>
    i === state.currentRound
      ? { pairs: r.pairs, results: [...r.results] }
      : r,
  );
  newRounds[state.currentRound].results[state.currentPairIndex] = decision;

  let promoted: number[];
  if (decision === "L") promoted = [pair.left];
  else if (decision === "R") promoted = [pair.right];
  else promoted = [pair.left, pair.right];

  const nextIdx = firstUndecidedPairIndex(newRounds[state.currentRound]);
  if (nextIdx !== -1) {
    return {
      ...state,
      rounds: newRounds,
      currentPairIndex: nextIdx,
      lastPromoted: promoted,
    };
  }

  // Round complete. Compute survivors, decide whether to spawn another round.
  const survivors = survivorsOfRound(newRounds[state.currentRound]);
  const prevSurvivors = state.currentRound === 0
    ? state.seedOrder
    : survivorsOfRound(newRounds[state.currentRound - 1]);

  if (survivors.length <= 1 || sameSet(survivors, prevSurvivors)) {
    return {
      ...state,
      rounds: newRounds,
      isComplete: true,
      lastPromoted: promoted,
    };
  }

  // Spawn next round by pairing survivors in their original seed order.
  const nextIds = state.seedOrder.filter((id) => survivors.includes(id));
  const nextRound = buildRoundFromIds(nextIds);
  newRounds.push(nextRound);
  const nextPairIdx = firstUndecidedPairIndex(nextRound);
  return {
    ...state,
    rounds: newRounds,
    currentRound: state.currentRound + 1,
    currentPairIndex: nextPairIdx === -1 ? nextRound.pairs.length : nextPairIdx,
    lastPromoted: promoted,
  };
}
