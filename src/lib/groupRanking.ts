/// Compute per-photo ranks within a group based on quality score and map
/// to a Narrative-Select-style tri-color dot (green / white / red).
///
/// Design choices:
/// - Only emit a color for groups with 2+ analyzed members. Single-photo
///   groups have nothing to rank; all-null groups mean AI hasn't run yet
///   and a dot would be misleading.
/// - Partition by rank percentile, not by absolute score. A group of
///   uniformly-sharp photos still gets a best and a worst.
/// - Photos with null qualityScore sort last (unknown = can't rank).

export type RankColor = "green" | "white" | "red";

export interface RankInput {
  id: number;
  qualityScore: number | null | undefined;
}

export interface RankResult {
  id: number;
  /// Rank 0 = best (highest quality) within the group. Null when the
  /// group only has one member (nothing to compare against) or nothing
  /// has been analyzed.
  rank: number | null;
  color: RankColor | null;
}

export function computeGroupRanks(members: RankInput[]): Map<number, RankResult> {
  const result = new Map<number, RankResult>();
  if (members.length < 2) {
    for (const m of members) {
      result.set(m.id, { id: m.id, rank: null, color: null });
    }
    return result;
  }

  // If nothing has been analyzed, skip.
  const anyAnalyzed = members.some(
    (m) => typeof m.qualityScore === "number",
  );
  if (!anyAnalyzed) {
    for (const m of members) {
      result.set(m.id, { id: m.id, rank: null, color: null });
    }
    return result;
  }

  // Stable sort: quality descending, unknowns last.
  const sorted = [...members].sort((a, b) => {
    const aq = typeof a.qualityScore === "number" ? a.qualityScore : -Infinity;
    const bq = typeof b.qualityScore === "number" ? b.qualityScore : -Infinity;
    return bq - aq;
  });

  const n = sorted.length;
  sorted.forEach((member, rank) => {
    // Unknowns (null/undefined score) get no color even if the group has
    // analyzed members — we'd be guessing.
    if (typeof member.qualityScore !== "number") {
      result.set(member.id, { id: member.id, rank: null, color: null });
      return;
    }
    const p = n === 1 ? 0 : rank / (n - 1);
    let color: RankColor;
    if (p < 0.34) color = "green";
    else if (p < 0.67) color = "white";
    else color = "red";
    result.set(member.id, { id: member.id, rank, color });
  });

  return result;
}

export function rankColorClass(color: RankColor | null): string | null {
  switch (color) {
    case "green":
      return "bg-emerald-500";
    case "white":
      return "bg-white";
    case "red":
      return "bg-red-500";
    default:
      return null;
  }
}
