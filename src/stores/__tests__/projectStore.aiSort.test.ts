import { describe, it, expect } from "vitest";
import { computeDisplayItems } from "../projectStore";
import type { ImageEntry, Group } from "../../types";

const img = (
  id: number,
  flag: string,
  sharp: number | null,
  faces: number | null = 0,
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
  flag,
  destination: "unrouted",
  starRating: 0,
  sharpnessScore: sharp,
  faceCount: faces,
  eyesOpenCount: 0,
  aiAnalyzedAt: sharp === null && faces === null ? null : "2026-04-16T00:00:00",
});

const g: Group[] = [];

describe("computeDisplayItems AI sort + filter", () => {
  it("sorts by sharpness descending, nulls last", () => {
    const images = [img(1, "pick", 40), img(2, "pick", 90), img(3, "pick", null)];
    const items = computeDisplayItems(images, "select", g, false, new Set(), true, 0, {
      sortByAi: "sharpness",
      hideSoftThreshold: 0,
    });
    expect(items.map((i) => i.image.id)).toEqual([2, 1, 3]);
  });

  it("sorts by faces descending, nulls last", () => {
    const images = [img(1, "pick", 50, 1), img(2, "pick", 50, 3), img(3, "pick", 50, null)];
    const items = computeDisplayItems(images, "select", g, false, new Set(), true, 0, {
      sortByAi: "faces",
      hideSoftThreshold: 0,
    });
    expect(items.map((i) => i.image.id)).toEqual([2, 1, 3]);
  });

  it("hideSoft hides below threshold but keeps nulls (opt-out while analyzing)", () => {
    const images = [img(1, "pick", 10), img(2, "pick", 50), img(3, "pick", null)];
    const items = computeDisplayItems(images, "select", g, false, new Set(), true, 0, {
      sortByAi: "none",
      hideSoftThreshold: 30,
    });
    expect(items.map((i) => i.image.id).sort()).toEqual([2, 3]);
  });

  it("hideSoft only applies in select + route views, not triage", () => {
    const images = [
      { ...img(1, "unreviewed", 10), starRating: 0 },
      { ...img(2, "unreviewed", 50), starRating: 0 },
    ];
    const items = computeDisplayItems(images, "triage", g, false, new Set(), false, 0, {
      sortByAi: "none",
      hideSoftThreshold: 30,
    });
    // Both still visible in triage — threshold only applies in select/route.
    expect(items.length).toBe(2);
  });

  it("sortByAi = none preserves view order", () => {
    const images = [img(1, "pick", 40), img(2, "pick", 90)];
    const items = computeDisplayItems(images, "select", g, false, new Set(), true, 0, {
      sortByAi: "none",
      hideSoftThreshold: 0,
    });
    expect(items.map((i) => i.image.id)).toEqual([1, 2]);
  });

  it("aiOptions argument is optional — existing 7-arg callers still work", () => {
    const images = [img(1, "pick", 40), img(2, "pick", 90)];
    const items = computeDisplayItems(images, "select", g, false, new Set(), true, 0);
    expect(items.map((i) => i.image.id)).toEqual([1, 2]);
  });
});
