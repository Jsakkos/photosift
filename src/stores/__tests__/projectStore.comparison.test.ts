import { useProjectStore } from "../projectStore";
import { computeDisplayItems } from "../projectStore";
import { setupMockIpc } from "../../test/mockIpc";
import { makeImage, makeShoot, makeGroup, resetIds } from "../../test/fixtures";

beforeEach(() => {
  resetIds();
  setupMockIpc();
});

/**
 * Helper: set up state in select view with images and groups, then
 * position the cursor at the given displayItem index.
 */
function setupSelectView(
  images: ReturnType<typeof makeImage>[],
  groups: ReturnType<typeof makeGroup>[],
  cursorIndex = 0,
) {
  const displayItems = computeDisplayItems(images, "select", groups);
  useProjectStore.setState({
    currentShoot: makeShoot(),
    images,
    groups,
    currentView: "select",
    viewMode: "sequential",
    currentIndex: cursorIndex,
    displayItems,
  });
  return displayItems;
}

describe("enterComparison", () => {
  test("from select with group sets comparison state", () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const img3 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
      { photoId: img3.id },
    ]);

    // In select view, group is expanded — img1, img2, img3 are all display items
    const displayItems = setupSelectView([img1, img2, img3], [group], 0);

    // Cursor is on img1 (first group member)
    expect(displayItems[0].image.id).toBe(img1.id);
    expect(displayItems[0].groupId).toBe(group.id);

    useProjectStore.getState().enterComparison();

    const state = useProjectStore.getState();
    expect(state.viewMode).toBe("comparison");
    expect(state.comparisonPinnedId).toBe(img1.id);
    expect(state.comparisonCyclingId).toBe(img2.id);
    expect(state.comparisonGroupMembers).toEqual([
      img1.id,
      img2.id,
      img3.id,
    ]);
  });

  test("from non-select view is a no-op", () => {
    const img1 = makeImage();
    const img2 = makeImage();
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
    ]);
    const displayItems = computeDisplayItems(
      [img1, img2],
      "triage",
      [group],
    );

    useProjectStore.setState({
      currentShoot: makeShoot(),
      images: [img1, img2],
      groups: [group],
      currentView: "triage",
      viewMode: "sequential",
      currentIndex: 0,
      displayItems,
    });

    useProjectStore.getState().enterComparison();

    expect(useProjectStore.getState().viewMode).toBe("sequential");
    expect(useProjectStore.getState().comparisonPinnedId).toBeNull();
  });

  test("on ungrouped item is a no-op", () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });

    // No groups — images are ungrouped in select view
    setupSelectView([img1, img2], [], 0);

    useProjectStore.getState().enterComparison();

    expect(useProjectStore.getState().viewMode).toBe("sequential");
    expect(useProjectStore.getState().comparisonPinnedId).toBeNull();
  });

  test("with fewer than 2 non-rejected members is a no-op", () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "reject" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
    ]);

    // Only img1 passes the flag !== "reject" filter, so < 2 members
    setupSelectView([img1, img2], [group], 0);

    useProjectStore.getState().enterComparison();

    expect(useProjectStore.getState().viewMode).toBe("sequential");
  });
});

describe("cycleComparison", () => {
  test("advances through members and wraps", () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const img3 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
      { photoId: img3.id },
    ]);

    setupSelectView([img1, img2, img3], [group], 0);
    useProjectStore.getState().enterComparison();

    // Pinned=img1, cycling starts at img2
    expect(useProjectStore.getState().comparisonCyclingId).toBe(img2.id);

    // Advance forward: img2 -> img3
    useProjectStore.getState().cycleComparison(1);
    expect(useProjectStore.getState().comparisonCyclingId).toBe(img3.id);

    // Advance forward again: wraps to img2 (skipping pinned img1)
    useProjectStore.getState().cycleComparison(1);
    expect(useProjectStore.getState().comparisonCyclingId).toBe(img2.id);
  });

  test("cycles backward and wraps", () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const img3 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
      { photoId: img3.id },
    ]);

    setupSelectView([img1, img2, img3], [group], 0);
    useProjectStore.getState().enterComparison();

    // Pinned=img1, cycling=img2. Go backward: wraps to img3
    useProjectStore.getState().cycleComparison(-1);
    expect(useProjectStore.getState().comparisonCyclingId).toBe(img3.id);
  });
});

describe("exitComparison", () => {
  test("resets all comparison state", () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
    ]);

    setupSelectView([img1, img2], [group], 0);
    useProjectStore.getState().enterComparison();

    expect(useProjectStore.getState().viewMode).toBe("comparison");

    useProjectStore.getState().exitComparison();

    const state = useProjectStore.getState();
    expect(state.viewMode).toBe("sequential");
    expect(state.comparisonPinnedId).toBeNull();
    expect(state.comparisonCyclingId).toBeNull();
    expect(state.comparisonGroupMembers).toEqual([]);
  });

  test("restores currentIndex to pinned image position", () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const img3 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
      { photoId: img3.id },
    ]);

    // Cursor on img2 (index 1 in the expanded group)
    setupSelectView([img1, img2, img3], [group], 1);
    useProjectStore.getState().enterComparison();

    // Pinned = img2 (the one at cursor when entering)
    expect(useProjectStore.getState().comparisonPinnedId).toBe(img2.id);

    useProjectStore.getState().exitComparison();

    // currentIndex should point to img2's position in displayItems
    const state = useProjectStore.getState();
    expect(state.displayItems[state.currentIndex].image.id).toBe(img2.id);
  });
});

describe("comparisonQuickPick", () => {
  test("left pick: picks pinned, rejects cycling", async () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const img3 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
      { photoId: img3.id },
    ]);

    setupSelectView([img1, img2, img3], [group], 0);
    useProjectStore.getState().enterComparison();

    // Pinned=img1, cycling=img2
    await useProjectStore.getState().comparisonQuickPick("left");

    const state = useProjectStore.getState();
    const updatedImg1 = state.images.find((i) => i.id === img1.id)!;
    const updatedImg2 = state.images.find((i) => i.id === img2.id)!;
    expect(updatedImg1.flag).toBe("pick");
    expect(updatedImg2.flag).toBe("reject");
  });

  test("right pick: picks cycling, rejects pinned", async () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const img3 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
      { photoId: img3.id },
    ]);

    setupSelectView([img1, img2, img3], [group], 0);
    useProjectStore.getState().enterComparison();

    // Pinned=img1, cycling=img2
    await useProjectStore.getState().comparisonQuickPick("right");

    const state = useProjectStore.getState();
    const updatedImg1 = state.images.find((i) => i.id === img1.id)!;
    const updatedImg2 = state.images.find((i) => i.id === img2.id)!;
    expect(updatedImg2.flag).toBe("pick");
    expect(updatedImg1.flag).toBe("reject");
  });

  test("exits comparison when no non-rejected members remain", async () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
    ]);

    setupSelectView([img1, img2], [group], 0);
    useProjectStore.getState().enterComparison();

    // Quick pick left: picks img1, rejects img2
    // No remaining non-rejected members besides pinned, so exits comparison
    await useProjectStore.getState().comparisonQuickPick("left");

    const state = useProjectStore.getState();
    expect(state.viewMode).toBe("sequential");
    expect(state.comparisonPinnedId).toBeNull();
  });

  test("advances cycling to next available member after rejection", async () => {
    const img1 = makeImage({ flag: "unreviewed" });
    const img2 = makeImage({ flag: "unreviewed" });
    const img3 = makeImage({ flag: "unreviewed" });
    const group = makeGroup([
      { photoId: img1.id, isCover: true },
      { photoId: img2.id },
      { photoId: img3.id },
    ]);

    setupSelectView([img1, img2, img3], [group], 0);
    useProjectStore.getState().enterComparison();

    // Pinned=img1, cycling=img2. Quick pick left rejects img2.
    await useProjectStore.getState().comparisonQuickPick("left");

    const state = useProjectStore.getState();
    // img2 is rejected, so cycling should advance to img3
    expect(state.viewMode).toBe("comparison");
    expect(state.comparisonCyclingId).toBe(img3.id);
  });
});
