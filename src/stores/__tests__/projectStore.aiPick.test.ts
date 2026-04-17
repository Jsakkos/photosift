import { describe, it, expect } from "vitest";
import { aiPickForGroup } from "../projectStore";
import type { ImageEntry, Group } from "../../types";

const e = (
  id: number,
  sharp: number | null,
  openCount: number | null,
): ImageEntry => ({
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
  flag: "pick",
  destination: "unrouted",
  starRating: 0,
  sharpnessScore: sharp,
  eyesOpenCount: openCount,
  faceCount: 0,
  aiAnalyzedAt: sharp === null ? null : "2026-04-16T00:00:00",
});

describe("aiPickForGroup", () => {
  const group: Group = {
    id: 1,
    shootId: 1,
    groupType: "near_duplicate",
    members: [
      { photoId: 1, isCover: false },
      { photoId: 2, isCover: false },
      { photoId: 3, isCover: false },
    ],
  };

  it("returns member with max sharpness * (1 + open eyes)", () => {
    const images = [e(1, 70, 2), e(2, 80, 0), e(3, 60, 4)];
    // Scores: 70*3=210, 80*1=80, 60*5=300 → pick 3.
    expect(aiPickForGroup(group, images)).toBe(3);
  });

  it("returns null when fewer than 2 analyzed members", () => {
    const images = [e(1, 70, 0), e(2, null, null)];
    expect(aiPickForGroup(group, images)).toBe(null);
  });

  it("breaks ties by lower id", () => {
    const images = [e(3, 50, 0), e(1, 50, 0), e(2, 50, 0)];
    expect(aiPickForGroup(group, images)).toBe(1);
  });

  it("returns null when group has fewer than 2 members total", () => {
    const tinyGroup: Group = {
      id: 2, shootId: 1, groupType: "near_duplicate",
      members: [{ photoId: 1, isCover: true }],
    };
    expect(aiPickForGroup(tinyGroup, [e(1, 90, 2)])).toBe(null);
  });
});
