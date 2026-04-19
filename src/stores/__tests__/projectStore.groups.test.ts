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

describe("setActiveInnerGroup", () => {
  test("activating opens the inner strip; passing null closes it; repeating same id is a no-op", () => {
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
      activeInnerGroupId: null,
    });

    // Collapsed: one cover item
    expect(useProjectStore.getState().displayItems).toHaveLength(1);

    useProjectStore.getState().setActiveInnerGroup(group.id);
    expect(useProjectStore.getState().activeInnerGroupId).toBe(group.id);
    // Drilled-in: 3 member rows
    expect(useProjectStore.getState().displayItems).toHaveLength(3);

    // Calling with the SAME id is a no-op (supports single-click-to-expand
    // without a repeated click accidentally collapsing).
    useProjectStore.getState().setActiveInnerGroup(group.id);
    expect(useProjectStore.getState().activeInnerGroupId).toBe(group.id);
    expect(useProjectStore.getState().displayItems).toHaveLength(3);

    // Passing null explicitly contracts.
    useProjectStore.getState().setActiveInnerGroup(null);
    expect(useProjectStore.getState().activeInnerGroupId).toBeNull();
    expect(useProjectStore.getState().displayItems).toHaveLength(1);
  });

  test("drilled-in displayItems contains ONLY that group's members — standalone photos and other groups are filtered out", () => {
    // Three groups, one standalone photo. Drilling into group 1 should
    // produce displayItems with exactly group 1's members, nothing else.
    const g1a = makeImage({ id: 1, flag: "unreviewed" });
    const g1b = makeImage({ id: 2, flag: "unreviewed" });
    const g2a = makeImage({ id: 3, flag: "unreviewed" });
    const g2b = makeImage({ id: 4, flag: "unreviewed" });
    const solo = makeImage({ id: 5, flag: "unreviewed" });
    const group1 = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
    ]);
    const group2 = makeGroup([
      { photoId: 3, isCover: true },
      { photoId: 4 },
    ]);
    // Force distinct ids (makeGroup uses a shared counter).
    (group2 as { id: number }).id = (group1 as { id: number }).id + 1;

    useProjectStore.setState({
      images: [g1a, g1b, g2a, g2b, solo],
      groups: [group1, group2],
      displayItems: computeDisplayItems(
        [g1a, g1b, g2a, g2b, solo],
        "triage",
        [group1, group2],
      ),
      currentView: "triage",
      currentIndex: 0,
      activeInnerGroupId: null,
    });

    // Collapsed: two covers + one standalone = 3 items.
    expect(useProjectStore.getState().displayItems).toHaveLength(3);

    useProjectStore.getState().setActiveInnerGroup(group1.id);
    const drilled = useProjectStore.getState().displayItems;
    // Drilled-in: exactly group 1's two members, no cover for group 2,
    // no standalone photo.
    expect(drilled).toHaveLength(2);
    expect(drilled.every((d) => d.groupId === group1.id)).toBe(true);
    expect(drilled.every((d) => !d.isGroupCover)).toBe(true);
    expect(drilled.map((d) => d.image.id).sort()).toEqual([1, 2]);
  });
});
