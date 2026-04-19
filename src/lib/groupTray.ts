import type { DisplayItem } from "../types";

/// Where a display item sits inside a contiguous run of expanded group
/// members. Used by `Filmstrip` to pick rounded corners, header
/// placement, and the VariableSizeList row height.
///
/// - `"none"`: not part of an expanded group (standalone photo, or a
///   collapsed group cover).
/// - `"solo"`: only one member of this group is present at this index
///   (edge case: other members filtered out by flag).
/// - `"first" | "middle" | "last"`: positional role within a run of 2+
///   members of the same group.
export type GroupTrayPosition = "none" | "solo" | "first" | "middle" | "last";

export function groupTrayPosition(
  items: DisplayItem[],
  index: number,
): GroupTrayPosition {
  const curr = items[index];
  if (!curr) return "none";
  if (curr.groupId === undefined) return "none";
  if (curr.isGroupCover) return "none";

  const prev = items[index - 1];
  const next = items[index + 1];
  const prevInGroup =
    !!prev && prev.groupId === curr.groupId && !prev.isGroupCover;
  const nextInGroup =
    !!next && next.groupId === curr.groupId && !next.isGroupCover;

  if (!prevInGroup && !nextInGroup) return "solo";
  if (!prevInGroup) return "first";
  if (!nextInGroup) return "last";
  return "middle";
}
