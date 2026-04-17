import { useProjectStore, computeDisplayItems } from "../projectStore";
import { setupMockIpc } from "../../test/mockIpc";
import { makeImage, makeShoot, makeGroup, resetIds } from "../../test/fixtures";

beforeEach(() => {
  resetIds();
});

describe("createGroupFromPhotos", () => {
  test("invokes IPC and refreshes groups", async () => {
    const img1 = makeImage({ id: 1 });
    const img2 = makeImage({ id: 2 });
    const img3 = makeImage({ id: 3 });

    const newGroup = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);

    const spy = vi.fn();
    setupMockIpc(
      {
        get_groups_for_shoot: [newGroup],
        create_group_from_photos: 99,
      },
      spy,
    );

    useProjectStore.setState({
      currentShoot: makeShoot({ id: 7 }),
      images: [img1, img2, img3],
      groups: [],
      displayItems: computeDisplayItems([img1, img2, img3], "triage", []),
      currentView: "triage",
      currentIndex: 0,
    });

    await useProjectStore.getState().createGroupFromPhotos([1, 2, 3]);

    const createCall = spy.mock.calls.find((c) => c[0] === "create_group_from_photos");
    expect(createCall).toBeDefined();
    expect((createCall![1] as { shootId: number; photoIds: number[] }).shootId).toBe(7);
    expect((createCall![1] as { shootId: number; photoIds: number[] }).photoIds).toEqual([1, 2, 3]);

    expect(useProjectStore.getState().groups).toHaveLength(1);
    // Triage now collapses to the cover
    const di = useProjectStore.getState().displayItems;
    expect(di).toHaveLength(1);
    expect(di[0].isGroupCover).toBe(true);
    expect(di[0].groupMemberCount).toBe(3);
  });

  test("refuses selections of fewer than 2 photos", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);

    useProjectStore.setState({
      currentShoot: makeShoot(),
      images: [],
      groups: [],
      displayItems: [],
    });

    await useProjectStore.getState().createGroupFromPhotos([5]);

    expect(spy.mock.calls.find((c) => c[0] === "create_group_from_photos")).toBeUndefined();
  });
});

describe("ungroupPhotos", () => {
  test("invokes IPC and refreshes groups", async () => {
    const img1 = makeImage({ id: 1 });
    const img2 = makeImage({ id: 2 });
    const img3 = makeImage({ id: 3 });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);

    const spy = vi.fn();
    setupMockIpc(
      {
        get_groups_for_shoot: [], // after ungroup, no groups left (or dissolved to < 2 members)
      },
      spy,
    );

    useProjectStore.setState({
      currentShoot: makeShoot({ id: 7 }),
      images: [img1, img2, img3],
      groups: [group],
      displayItems: computeDisplayItems([img1, img2, img3], "triage", [group]),
      currentView: "triage",
      currentIndex: 0,
    });

    await useProjectStore.getState().ungroupPhotos([1, 2, 3]);

    const ungroupCall = spy.mock.calls.find((c) => c[0] === "ungroup_photos");
    expect(ungroupCall).toBeDefined();
    expect((ungroupCall![1] as { photoIds: number[] }).photoIds).toEqual([1, 2, 3]);

    expect(useProjectStore.getState().groups).toHaveLength(0);
    // Triage now shows all 3 ungrouped photos
    expect(useProjectStore.getState().displayItems).toHaveLength(3);
  });
});

describe("toggleGroupExpansion", () => {
  test("toggling adds and removes groupId and rebuilds displayItems", () => {
    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const img2 = makeImage({ id: 2, flag: "unreviewed" });
    const img3 = makeImage({ id: 3, flag: "unreviewed" });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);

    useProjectStore.setState({
      images: [img1, img2, img3],
      groups: [group],
      displayItems: computeDisplayItems([img1, img2, img3], "triage", [group]),
      currentView: "triage",
      currentIndex: 0,
      expandedGroupIds: new Set<number>(),
    });

    // Collapsed: one cover item
    expect(useProjectStore.getState().displayItems).toHaveLength(1);

    useProjectStore.getState().toggleGroupExpansion(group.id);
    expect(useProjectStore.getState().expandedGroupIds.has(group.id)).toBe(true);
    // Expanded: 3 member rows
    expect(useProjectStore.getState().displayItems).toHaveLength(3);

    useProjectStore.getState().toggleGroupExpansion(group.id);
    expect(useProjectStore.getState().expandedGroupIds.has(group.id)).toBe(false);
    expect(useProjectStore.getState().displayItems).toHaveLength(1);
  });
});
