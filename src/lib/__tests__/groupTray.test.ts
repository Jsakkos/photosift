import { describe, it, expect } from "vitest";
import { groupTrayPosition } from "../groupTray";
import type { DisplayItem, ImageEntry } from "../../types";

function img(id: number): ImageEntry {
  return {
    id,
    filepath: `/fake/${id}.nef`,
    filename: `${id}.nef`,
    captureTime: null,
    cameraModel: null,
    lens: null,
    focalLength: null,
    aperture: null,
    shutterSpeed: null,
    iso: null,
    flag: "unreviewed",
    destination: "unrouted",
    starRating: 0,
    sharpnessScore: null,
    eyesOpenCount: null,
    faceCount: 0,
    aiAnalyzedAt: null,
  };
}

function di(id: number, overrides: Partial<DisplayItem> = {}): DisplayItem {
  return { imageIndex: id, image: img(id), ...overrides };
}

describe("groupTrayPosition", () => {
  it("standalone photo returns none", () => {
    const items = [di(1)];
    expect(groupTrayPosition(items, 0)).toBe("none");
  });

  it("collapsed group cover returns none — GroupStack handles styling", () => {
    const items = [di(1, { groupId: 5, isGroupCover: true, groupMemberCount: 3 })];
    expect(groupTrayPosition(items, 0)).toBe("none");
  });

  it("solo expanded member (neighbors not in group) returns solo", () => {
    const items = [
      di(1),
      di(2, { groupId: 5 }),
      di(3),
    ];
    expect(groupTrayPosition(items, 1)).toBe("solo");
  });

  it("three consecutive group members return first, middle, last", () => {
    const items = [
      di(1),
      di(2, { groupId: 5 }),
      di(3, { groupId: 5 }),
      di(4, { groupId: 5 }),
      di(5),
    ];
    expect(groupTrayPosition(items, 1)).toBe("first");
    expect(groupTrayPosition(items, 2)).toBe("middle");
    expect(groupTrayPosition(items, 3)).toBe("last");
  });

  it("two members return first and last, no middle", () => {
    const items = [
      di(1, { groupId: 5 }),
      di(2, { groupId: 5 }),
    ];
    expect(groupTrayPosition(items, 0)).toBe("first");
    expect(groupTrayPosition(items, 1)).toBe("last");
  });

  it("different group next to current member breaks the run", () => {
    // Two groups adjacent: group 5 ends at index 1, group 7 starts at 2.
    const items = [
      di(1, { groupId: 5 }),
      di(2, { groupId: 5 }),
      di(3, { groupId: 7 }),
      di(4, { groupId: 7 }),
    ];
    expect(groupTrayPosition(items, 1)).toBe("last");
    expect(groupTrayPosition(items, 2)).toBe("first");
  });

  it("collapsed cover adjacent to expanded member does not extend the run", () => {
    // A collapsed cover has isGroupCover=true — it's a GroupStack, not
    // part of the expanded tray, so it must not bridge two runs.
    const items = [
      di(1, { groupId: 5 }),
      di(2, { groupId: 5, isGroupCover: true, groupMemberCount: 1 }),
      di(3, { groupId: 5 }),
    ];
    expect(groupTrayPosition(items, 0)).toBe("solo");
    expect(groupTrayPosition(items, 2)).toBe("solo");
  });

  it("out-of-bounds index returns none", () => {
    const items = [di(1, { groupId: 5 })];
    expect(groupTrayPosition(items, -1)).toBe("none");
    expect(groupTrayPosition(items, 99)).toBe("none");
  });
});
