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

    test("collapses group to cover with correct member count", () => {
      const img1 = makeImage({ id: 1 });
      const img2 = makeImage({ id: 2 });
      const img3 = makeImage({ id: 3 });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "triage", [group]);

      expect(items).toHaveLength(1);
      expect(items[0].image.id).toBe(1);
      expect(items[0].isGroupCover).toBe(true);
      expect(items[0].groupMemberCount).toBe(3);
      expect(items[0].groupId).toBe(group.id);
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

    test("shows ungrouped alongside grouped items", () => {
      const img1 = makeImage({ id: 1 });
      const img2 = makeImage({ id: 2 });
      const img3 = makeImage({ id: 3 });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "triage", [group]);

      expect(items).toHaveLength(2);
      expect(items[0].isGroupCover).toBe(true);
      expect(items[1].image.id).toBe(3);
      expect(items[1].isGroupCover).toBeUndefined();
    });

    test("uses isCover member as the displayed cover image", () => {
      const img1 = makeImage({ id: 1 });
      const img2 = makeImage({ id: 2 });
      const group = makeGroup([
        { photoId: 1, isCover: false },
        { photoId: 2, isCover: true },
      ]);

      const items = computeDisplayItems([img1, img2], "triage", [group]);

      expect(items).toHaveLength(1);
      expect(items[0].image.id).toBe(2);
    });

    test("groupMemberCount reflects total members, not just unreviewed", () => {
      const img1 = makeImage({ id: 1, flag: "unreviewed" });
      const img2 = makeImage({ id: 2, flag: "pick" });
      const img3 = makeImage({ id: 3, flag: "unreviewed" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "triage", [group]);

      expect(items).toHaveLength(1);
      expect(items[0].groupMemberCount).toBe(3);
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

  describe("edge cases", () => {
    test("empty images returns empty for all views", () => {
      expect(computeDisplayItems([], "triage", [])).toHaveLength(0);
      expect(computeDisplayItems([], "select", [])).toHaveLength(0);
      expect(computeDisplayItems([], "route", [])).toHaveLength(0);
    });
  });
});
