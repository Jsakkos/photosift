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
    // Triage shows every photo flat — grouping no longer collapses the
    // list. All 3 members appear, each carrying the new group id.
    const di = useProjectStore.getState().displayItems;
    expect(di).toHaveLength(3);
    expect(di.every((d) => d.groupId === newGroup.id)).toBe(true);
    expect(di.every((d) => d.isGroupCover === undefined)).toBe(true);
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
  test("drilling into a group filters Select's displayItems to its members", () => {
    // Group drill-in is a Select-view feature (Triage is a flat per-photo
    // pass). Drilling narrows the list to one group; null restores it.
    const g1 = makeImage({ id: 1, flag: "pick" });
    const g2 = makeImage({ id: 2, flag: "pick" });
    const g3 = makeImage({ id: 3, flag: "pick" });
    const solo = makeImage({ id: 4, flag: "pick" });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);

    useProjectStore.setState({
      images: [g1, g2, g3, solo],
      groups: [group],
      displayItems: computeDisplayItems([g1, g2, g3, solo], "select", [group]),
      currentView: "select",
      currentIndex: 0,
      activeInnerGroupId: null,
    });

    // Select expands all members inline: 3 grouped + 1 standalone = 4.
    expect(useProjectStore.getState().displayItems).toHaveLength(4);

    // Drilling in narrows to just that group's members.
    useProjectStore.getState().setActiveInnerGroup(group.id);
    expect(useProjectStore.getState().activeInnerGroupId).toBe(group.id);
    const drilled = useProjectStore.getState().displayItems;
    expect(drilled).toHaveLength(3);
    expect(drilled.every((d) => d.groupId === group.id)).toBe(true);

    // Passing null restores the full expanded list.
    useProjectStore.getState().setActiveInnerGroup(null);
    expect(useProjectStore.getState().activeInnerGroupId).toBeNull();
    expect(useProjectStore.getState().displayItems).toHaveLength(4);
  });
});
