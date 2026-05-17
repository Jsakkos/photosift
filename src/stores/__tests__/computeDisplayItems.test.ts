import { computeDisplayItems } from "../projectStore";
import { makeImage, makeGroup, resetIds } from "../../test/fixtures";

beforeEach(() => {
  resetIds();
});

describe("computeDisplayItems", () => {
  describe("triage view", () => {
    test("shows ungrouped unreviewed images", () => {
      const images = [
        makeImage({ flag: "unreviewed" }),
        makeImage({ flag: "pick" }),
        makeImage({ flag: "unreviewed" }),
      ];

      const items = computeDisplayItems(images, "triage", []);

      expect(items).toHaveLength(2);
      expect(items[0].image.id).toBe(1);
      expect(items[1].image.id).toBe(3);
    });

    test("emits every group member individually — no cover collapsing", () => {
      const img1 = makeImage({ id: 1 });
      const img2 = makeImage({ id: 2 });
      const img3 = makeImage({ id: 3 });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "triage", [group]);

      // Triage reviews every photo individually — N members = N items.
      expect(items).toHaveLength(3);
      expect(items.map((i) => i.image.id)).toEqual([1, 2, 3]);
      // groupId is still attached (Shift+P "keep group" relies on it),
      // but there is no cover / member-count metadata in triage.
      expect(items.every((i) => i.groupId === group.id)).toBe(true);
      expect(items.every((i) => i.isGroupCover === undefined)).toBe(true);
      expect(items.every((i) => i.groupMemberCount === undefined)).toBe(true);
    });

    test("excludes group when all members are reviewed", () => {
      const img1 = makeImage({ id: 1, flag: "pick" });
      const img2 = makeImage({ id: 2, flag: "reject" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
      ]);

      const items = computeDisplayItems([img1, img2], "triage", [group]);

      expect(items).toHaveLength(0);
    });

    test("shows ungrouped alongside grouped items, all flat", () => {
      const img1 = makeImage({ id: 1 });
      const img2 = makeImage({ id: 2 });
      const img3 = makeImage({ id: 3 });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "triage", [group]);

      expect(items).toHaveLength(3);
      expect(items.map((i) => i.image.id)).toEqual([1, 2, 3]);
      expect(items[0].groupId).toBe(group.id);
      expect(items[2].groupId).toBeUndefined();
    });

    test("skips non-unreviewed members of a group, keeps the rest flat", () => {
      const img1 = makeImage({ id: 1, flag: "unreviewed" });
      const img2 = makeImage({ id: 2, flag: "reject" });
      const img3 = makeImage({ id: 3, flag: "unreviewed" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "triage", [group]);

      expect(items).toHaveLength(2);
      expect(items.map((i) => i.image.id)).toEqual([1, 3]);
    });
  });

  describe("select view", () => {
    test("expands group members, excludes rejects", () => {
      const img1 = makeImage({ id: 1, flag: "pick" });
      const img2 = makeImage({ id: 2, flag: "unreviewed" });
      const img3 = makeImage({ id: 3, flag: "reject" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "select", [group]);

      expect(items).toHaveLength(2);
      expect(items.map((i) => i.image.id)).toEqual([1, 2]);
      expect(items[0].groupId).toBe(group.id);
    });

    test("shows ungrouped non-rejected images", () => {
      const images = [
        makeImage({ flag: "unreviewed" }),
        makeImage({ flag: "reject" }),
        makeImage({ flag: "pick" }),
      ];

      const items = computeDisplayItems(images, "select", []);

      expect(items).toHaveLength(2);
      expect(items.map((i) => i.image.id)).toEqual([1, 3]);
    });

    test("excludes entire group when all members rejected", () => {
      const img1 = makeImage({ id: 1, flag: "reject" });
      const img2 = makeImage({ id: 2, flag: "reject" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
      ]);

      const items = computeDisplayItems([img1, img2], "select", [group]);

      expect(items).toHaveLength(0);
    });
  });

  describe("route view", () => {
    test("shows only picked + unrouted images", () => {
      const images = [
        makeImage({ flag: "pick", destination: "unrouted" }),
        makeImage({ flag: "pick", destination: "edit" }),
        makeImage({ flag: "unreviewed", destination: "unrouted" }),
        makeImage({ flag: "pick", destination: "unrouted" }),
      ];

      const items = computeDisplayItems(images, "route", []);

      expect(items).toHaveLength(2);
      expect(items.map((i) => i.image.id)).toEqual([1, 4]);
    });

    test("returns empty when no picks exist", () => {
      const images = [
        makeImage({ flag: "unreviewed" }),
        makeImage({ flag: "reject" }),
      ];

      const items = computeDisplayItems(images, "route", []);

      expect(items).toHaveLength(0);
    });

    test("ignores groups in route view", () => {
      const img1 = makeImage({ id: 1, flag: "pick", destination: "unrouted" });
      const img2 = makeImage({ id: 2, flag: "pick", destination: "unrouted" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
      ]);

      const items = computeDisplayItems([img1, img2], "route", [group]);

      expect(items).toHaveLength(2);
      expect(items[0].groupId).toBeUndefined();
    });
  });

  describe("select view gate (selectRequiresPick)", () => {
    test("selectRequiresPick=true drops unreviewed members from group", () => {
      const img1 = makeImage({ id: 1, flag: "pick" });
      const img2 = makeImage({ id: 2, flag: "unreviewed" });
      const img3 = makeImage({ id: 3, flag: "reject" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems(
        [img1, img2, img3],
        "select",
        [group],
        new Set(),
        true, // selectRequiresPick
      );

      expect(items).toHaveLength(1);
      expect(items[0].image.id).toBe(1);
    });

    test("selectRequiresPick=true drops ungrouped unreviewed images", () => {
      const images = [
        makeImage({ flag: "unreviewed" }),
        makeImage({ flag: "pick" }),
        makeImage({ flag: "reject" }),
      ];

      const items = computeDisplayItems(
        images,
        "select",
        [],
        new Set(),
        true,
      );

      expect(items).toHaveLength(1);
      expect(items[0].image.id).toBe(2);
    });

    test("selectRequiresPick=false preserves legacy flag != reject semantics", () => {
      const images = [
        makeImage({ flag: "unreviewed" }),
        makeImage({ flag: "pick" }),
      ];

      const items = computeDisplayItems(
        images,
        "select",
        [],
        new Set(),
        false,
      );

      expect(items).toHaveLength(2);
    });
  });

  describe("route view gate (routeMinStar)", () => {
    test("routeMinStar=3 drops picks below threshold", () => {
      const images = [
        makeImage({ flag: "pick", destination: "unrouted", starRating: 0 }),
        makeImage({ flag: "pick", destination: "unrouted", starRating: 2 }),
        makeImage({ flag: "pick", destination: "unrouted", starRating: 3 }),
        makeImage({ flag: "pick", destination: "unrouted", starRating: 5 }),
      ];

      const items = computeDisplayItems(
        images,
        "route",
        [],
        new Set(),
        false,
        3, // routeMinStar
      );

      expect(items).toHaveLength(2);
      expect(items.map((i) => i.image.starRating).sort()).toEqual([3, 5]);
    });

    test("routeMinStar=0 disables the gate", () => {
      const images = [
        makeImage({ flag: "pick", destination: "unrouted", starRating: 0 }),
        makeImage({ flag: "pick", destination: "unrouted", starRating: 3 }),
      ];

      const items = computeDisplayItems(
        images,
        "route",
        [],
        new Set(),
        false,
        0,
      );

      expect(items).toHaveLength(2);
    });
  });

  describe("edge cases", () => {
    test("empty images returns empty for all views", () => {
      expect(computeDisplayItems([], "triage", [])).toHaveLength(0);
      expect(computeDisplayItems([], "select", [])).toHaveLength(0);
      expect(computeDisplayItems([], "route", [])).toHaveLength(0);
    });
  });
});
