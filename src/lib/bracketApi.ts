// Typed wrappers around the bracket-decision Tauri commands. Tournament
// picks persist here so the Review tab can replay them.

import { invoke } from "@tauri-apps/api/core";
import type { BracketDecision, BracketDecisionValue } from "../types";

/// Every persisted bracket decision for a shoot — user picks and the
/// Curator-derived ones.
export async function getBracketDecisionsForShoot(
  shootId: number,
): Promise<BracketDecision[]> {
  return await invoke<BracketDecision[]>("get_bracket_decisions_for_shoot", {
    shootId,
  });
}

/// Persist one user tournament decision. Fire-and-forget from the Select
/// bracket — idempotent on `(group, round, pair)`.
export async function recordBracketDecision(
  shootId: number,
  groupId: number,
  roundIndex: number,
  pairIndex: number,
  leftPhotoId: number,
  rightPhotoId: number | null,
  decision: BracketDecisionValue,
): Promise<void> {
  await invoke("record_bracket_decision", {
    shootId,
    groupId,
    roundIndex,
    pairIndex,
    leftPhotoId,
    rightPhotoId,
    decision,
  });
}

/// Remove one user bracket decision — used when a tournament pick is undone.
export async function deleteBracketDecision(
  groupId: number,
  roundIndex: number,
  pairIndex: number,
): Promise<void> {
  await invoke("delete_bracket_decision", { groupId, roundIndex, pairIndex });
}
