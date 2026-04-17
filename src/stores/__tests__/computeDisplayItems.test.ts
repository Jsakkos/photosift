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

    test("triageExpandGroups=true shows every group member individually", () => {
      const img1 = makeImage({ id: 1, flag: "unreviewed" });
      const img2 = makeImage({ id: 2, flag: "unreviewed" });
      const img3 = makeImage({ id: 3, flag: "unreviewed" });
      const ungrouped = makeImage({ id: 4, flag: "unreviewed" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const collapsed = computeDisplayItems(
        [img1, img2, img3, ungrouped],
        "triage",
        [group],
        false,
      );
      // Default: 1 cover + 1 ungrouped = 2 items
      expect(collapsed).toHaveLength(2);

      const expanded = computeDisplayItems(
        [img1, img2, img3, ungrouped],
        "triage",
        [group],
        true,
      );
      // Expanded: 3 group members + 1 ungrouped = 4 items
      expect(expanded).toHaveLength(4);
      // All three group items carry groupId for the blue-bar affiliation
      expect(expanded[0].groupId).toBe(group.id);
      expect(expanded[1].groupId).toBe(group.id);
      expect(expanded[2].groupId).toBe(group.id);
      expect(expanded[3].groupId).toBeUndefined();
      // Expanded items do not advertise isGroupCover (no +N badge in this mode)
      expect(expanded[0].isGroupCover).toBeUndefined();
    });

    test("triageExpandGroups only shows unreviewed members (skips picks/rejects)", () => {
      const img1 = makeImage({ id: 1, flag: "unreviewed" });
      const img2 = makeImage({ id: 2, flag: "pick" });
      const img3 = makeImage({ id: 3, flag: "reject" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems([img1, img2, img3], "triage", [group], true);

      expect(items).toHaveLength(1);
      expect(items[0].image.id).toBe(1);
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

    test("expandedGroupIds drills into one group inline while others stay collapsed", () => {
      const img1 = makeImage({ id: 1, flag: "unreviewed" });
      const img2 = makeImage({ id: 2, flag: "unreviewed" });
      const img3 = makeImage({ id: 3, flag: "unreviewed" });
      const img4 = makeImage({ id: 4, flag: "unreviewed" });
      const img5 = makeImage({ id: 5, flag: "unreviewed" });
      const groupA = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
      ]);
      const groupB = makeGroup([
        { photoId: 3, isCover: true },
        { photoId: 4 },
        { photoId: 5 },
      ]);

      const items = computeDisplayItems(
        [img1, img2, img3, img4, img5],
        "triage",
        [groupA, groupB],
        false,
        new Set([groupB.id]),
      );

      // groupA stays collapsed (1 cover) + groupB expanded (3 members) = 4 items
      expect(items).toHaveLength(4);
      expect(items[0].isGroupCover).toBe(true);
      expect(items[0].groupId).toBe(groupA.id);
      // Expanded members carry groupId but no isGroupCover / count
      expect(items[1].groupId).toBe(groupB.id);
      expect(items[1].isGroupCover).toBeUndefined();
      expect(items.slice(1).map((i) => i.image.id)).toEqual([3, 4, 5]);
    });

    test("expandedGroupIds skips non-unreviewed members within an expanded group", () => {
      const img1 = makeImage({ id: 1, flag: "unreviewed" });
      const img2 = makeImage({ id: 2, flag: "reject" });
      const img3 = makeImage({ id: 3, flag: "unreviewed" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
        { photoId: 3 },
      ]);

      const items = computeDisplayItems(
        [img1, img2, img3],
        "triage",
        [group],
        false,
        new Set([group.id]),
      );

      expect(items).toHaveLength(2);
      expect(items.map((i) => i.image.id)).toEqual([1, 3]);
    });

    test("global triageExpandGroups=true makes expandedGroupIds a no-op", () => {
      const img1 = makeImage({ id: 1, flag: "unreviewed" });
      const img2 = makeImage({ id: 2, flag: "unreviewed" });
      const group = makeGroup([
        { photoId: 1, isCover: true },
        { photoId: 2 },
      ]);

      const withExpanded = computeDisplayItems(
        [img1, img2],
        "triage",
        [group],
        true,
        new Set([group.id]),
      );
      const withoutExpanded = computeDisplayItems(
        [img1, img2],
        "triage",
        [group],
        true,
        new Set(),
      );
      expect(withExpanded).toHaveLength(2);
      expect(withoutExpanded).toHaveLength(2);
      expect(withExpanded.map((i) => i.image.id)).toEqual(
        withoutExpanded.map((i) => i.image.id),
      );
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
        false,
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
        false,
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
        false,
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
        false,
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
        false,
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
