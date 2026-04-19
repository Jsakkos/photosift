import { describe, it, expect } from "vitest";
import { aiPickForGroup, computeDisplayItems } from "../projectStore";
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

  it("returns member with max sharpness * (1 + open eyes) when useEyes is true", () => {
    const images = [e(1, 70, 2), e(2, 80, 0), e(3, 60, 4)];
    // Scores: 70*3=210, 80*1=80, 60*5=300 → pick 3.
    expect(aiPickForGroup(group, images, true)).toBe(3);
  });

  it("defaults to sharpness-only when useEyes is omitted (mock eye provider)", () => {
    const images = [e(1, 70, 2), e(2, 80, 0), e(3, 60, 4)];
    // Sharpness alone: 80 > 70 > 60 → pick 2, ignoring mock-noisy eye counts.
    expect(aiPickForGroup(group, images)).toBe(2);
  });

  it("returns null when fewer than 2 analyzed members", () => {
    const images = [e(1, 70, 0), e(2, null, null)];
    expect(aiPickForGroup(group, images, true)).toBe(null);
  });

  it("breaks ties by lower id", () => {
    const images = [e(3, 50, 0), e(1, 50, 0), e(2, 50, 0)];
    expect(aiPickForGroup(group, images, true)).toBe(1);
  });

  it("returns null when group has fewer than 2 members total", () => {
    const tinyGroup: Group = {
      id: 2, shootId: 1, groupType: "near_duplicate",
      members: [{ photoId: 1, isCover: true }],
    };
    expect(aiPickForGroup(tinyGroup, [e(1, 90, 2)], true)).toBe(null);
  });
});

describe("computeDisplayItems — AI pick stays visible in expanded triage", () => {
  const baseGroup: Group = {
    id: 7,
    shootId: 1,
    groupType: "near_duplicate",
    members: [
      { photoId: 10, isCover: true },
      { photoId: 11, isCover: false },
      { photoId: 12, isCover: false },
    ],
  };

  // Photo 11 is the pick (highest sharpness). We set its flag to `pick`
  // while leaving siblings unreviewed — regression case for "badge
  // disappears once the AI-recommended photo is flagged while the group
  // is expanded."
  const picked = (flag: string): ImageEntry =>
    ({ ...e(11, 90, 0), flag }) as ImageEntry;

  it("expanded group: pick member stays in display items even when already flagged", () => {
    const images = [
      { ...e(10, 50, 0), flag: "unreviewed" },
      picked("pick"),
      { ...e(12, 40, 0), flag: "unreviewed" },
    ];
    const items = computeDisplayItems(
      images,
      "triage",
      [baseGroup],
      false,
      new Set([7]), // group 7 expanded
      false,
      0,
      { sortByAi: "none", hideSoftThreshold: 0, useEyesInPick: false },
    );
    const pickItem = items.find((d) => d.image.id === 11);
    expect(pickItem, "pick member must be present even though flag=pick").toBeDefined();
    expect(pickItem!.isAiPick).toBe(true);
  });

  it("triageExpandGroups=true: rejected pick is hidden", () => {
    // Regression: previously the isPinnedPick override kept an AI pick
    // visible even after it was rejected, which left the badge stuck on
    // a photo the user had already culled. Rejected means gone — no
    // exception for AI picks.
    const images = [
      { ...e(10, 50, 0), flag: "unreviewed" },
      picked("reject"),
      { ...e(12, 40, 0), flag: "unreviewed" },
    ];
    const items = computeDisplayItems(
      images,
      "triage",
      [baseGroup],
      true, // triageExpandGroups
      new Set(),
      false,
      0,
      { sortByAi: "none", hideSoftThreshold: 0, useEyesInPick: false },
    );
    expect(
      items.find((d) => d.image.id === 11),
      "rejected pick must be filtered out",
    ).toBeUndefined();
  });

  it("drill-down (expandedGroupIds): rejected pick is hidden", () => {
    // Same bug surfaces in the non-triageExpandGroups branch when the
    // user has manually drilled into a single group. Rejected picks
    // must drop out here too.
    const images = [
      { ...e(10, 50, 0), flag: "unreviewed" },
      picked("reject"),
      { ...e(12, 40, 0), flag: "unreviewed" },
    ];
    const items = computeDisplayItems(
      images,
      "triage",
      [baseGroup],
      false,
      new Set([7]), // group 7 expanded via drill-down
      false,
      0,
      { sortByAi: "none", hideSoftThreshold: 0, useEyesInPick: false },
    );
    expect(
      items.find((d) => d.image.id === 11),
      "rejected pick must be filtered out in drill-down view too",
    ).toBeUndefined();
  });

  it("non-pick flagged members stay hidden in expanded state", () => {
    // Photo 12 has the lowest sharpness — not the pick — and is flagged.
    // It should drop out of the expanded view as before.
    const images = [
      { ...e(10, 50, 0), flag: "unreviewed" },
      { ...e(11, 90, 0), flag: "unreviewed" },
      { ...e(12, 40, 0), flag: "reject" },
    ];
    const items = computeDisplayItems(
      images,
      "triage",
      [baseGroup],
      false,
      new Set([7]),
      false,
      0,
      { sortByAi: "none", hideSoftThreshold: 0, useEyesInPick: false },
    );
    expect(items.find((d) => d.image.id === 12)).toBeUndefined();
    expect(items.find((d) => d.image.id === 11)?.isAiPick).toBe(true);
  });
});
