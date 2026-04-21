import { useProjectStore } from "../projectStore";
import { computeDisplayItems } from "../projectStore";
import { setupMockIpc } from "../../test/mockIpc";
import {
  makeImage,
  makeGroup,
  resetIds,
} from "../../test/fixtures";

beforeEach(() => {
  resetIds();
});

describe("setFlag", () => {
  test("select view: P auto-rejects siblings", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);

    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const img2 = makeImage({ id: 2, flag: "unreviewed" });
    const img3 = makeImage({ id: 3, flag: "unreviewed" });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);

    const images = [img1, img2, img3];
    const groups = [group];
    const displayItems = computeDisplayItems(images, "select", groups);

    // In select view, all 3 non-rejected members are expanded
    expect(displayItems).toHaveLength(3);

    useProjectStore.setState({
      images,
      groups,
      displayItems,
      currentView: "select",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlag("pick");

    const state = useProjectStore.getState();
    const flagged = state.images;

    // Current image (id=1) should be picked
    expect(flagged.find((i) => i.id === 1)!.flag).toBe("pick");
    // Siblings (id=2, id=3) should be auto-rejected
    expect(flagged.find((i) => i.id === 2)!.flag).toBe("reject");
    expect(flagged.find((i) => i.id === 3)!.flag).toBe("reject");

    // Verify bulk_set_flag was called with sibling IDs
    const bulkCall = spy.mock.calls.find(
      (call) => call[0] === "bulk_set_flag",
    );
    expect(bulkCall).toBeDefined();
    const bulkArgs = bulkCall![1] as { photoIds: number[]; flag: string };
    expect(bulkArgs.photoIds).toEqual(expect.arrayContaining([2, 3]));
    expect(bulkArgs.photoIds).toHaveLength(2);
    expect(bulkArgs.flag).toBe("reject");

    // Undo stack should have one entry carrying all 3 members (1 pick + 2 rejects)
    expect(state.undoStack).toHaveLength(1);
    const entry = state.undoStack[0];
    expect(entry.batch).toHaveLength(3);
    const byId = new Map(entry.batch!.map((b) => [b.imageId, b]));
    expect(byId.get(1)).toMatchObject({ oldValue: "unreviewed", newValue: "pick" });
    expect(byId.get(2)).toMatchObject({ oldValue: "unreviewed", newValue: "reject" });
    expect(byId.get(3)).toMatchObject({ oldValue: "unreviewed", newValue: "reject" });
  });

  test("select view: Shift+P (setFlagNoAutoReject) keeps siblings", async () => {
    setupMockIpc({});

    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const img2 = makeImage({ id: 2, flag: "unreviewed" });
    const img3 = makeImage({ id: 3, flag: "unreviewed" });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);

    const images = [img1, img2, img3];
    const groups = [group];
    const displayItems = computeDisplayItems(images, "select", groups);

    useProjectStore.setState({
      images,
      groups,
      displayItems,
      currentView: "select",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlagNoAutoReject("pick");

    const state = useProjectStore.getState();
    const flagged = state.images;

    // Current image is picked
    expect(flagged.find((i) => i.id === 1)!.flag).toBe("pick");
    // Siblings remain unreviewed
    expect(flagged.find((i) => i.id === 2)!.flag).toBe("unreviewed");
    expect(flagged.find((i) => i.id === 3)!.flag).toBe("unreviewed");
  });

  test("triage view: P on group cover flags ALL members", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);

    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const img2 = makeImage({ id: 2, flag: "unreviewed" });
    const img3 = makeImage({ id: 3, flag: "unreviewed" });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);

    const images = [img1, img2, img3];
    const groups = [group];
    const displayItems = computeDisplayItems(images, "triage", groups);

    // Triage collapses group to one cover item
    expect(displayItems).toHaveLength(1);
    expect(displayItems[0].isGroupCover).toBe(true);

    useProjectStore.setState({
      images,
      groups,
      displayItems,
      currentView: "triage",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlag("pick");

    const state = useProjectStore.getState();
    const flagged = state.images;

    // All group members should be picked
    expect(flagged.find((i) => i.id === 1)!.flag).toBe("pick");
    expect(flagged.find((i) => i.id === 2)!.flag).toBe("pick");
    expect(flagged.find((i) => i.id === 3)!.flag).toBe("pick");

    // Verify bulk_set_flag was called with all member IDs
    const bulkCall = spy.mock.calls.find(
      (call) => call[0] === "bulk_set_flag",
    );
    expect(bulkCall).toBeDefined();
    const bulkArgs = bulkCall![1] as { photoIds: number[]; flag: string };
    expect(bulkArgs.photoIds).toEqual(expect.arrayContaining([1, 2, 3]));
    expect(bulkArgs.flag).toBe("pick");

    // Undo entry carries all 3 members so undo can restore each one
    expect(state.undoStack).toHaveLength(1);
    expect(state.undoStack[0].batch).toHaveLength(3);
    expect(state.undoStack[0].imageId).toBe(1); // cover image (primary)
    const ids = state.undoStack[0].batch!.map((b) => b.imageId).sort();
    expect(ids).toEqual([1, 2, 3]);
  });

  test("single ungrouped image flag", async () => {
    setupMockIpc({});

    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const images = [img1];
    const displayItems = computeDisplayItems(images, "triage", []);

    useProjectStore.setState({
      images,
      groups: [],
      displayItems,
      currentView: "triage",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlag("reject");

    const state = useProjectStore.getState();
    expect(state.images[0].flag).toBe("reject");
    expect(state.undoStack).toHaveLength(1);
    expect(state.undoStack[0].oldValue).toBe("unreviewed");
    expect(state.undoStack[0].newValue).toBe("reject");
  });

  test("setFlag is a no-op when flag matches current value", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);

    const img1 = makeImage({ id: 1, flag: "pick" });
    const images = [img1];
    const displayItems = computeDisplayItems(images, "select", []);

    useProjectStore.setState({
      images,
      groups: [],
      displayItems,
      currentView: "select",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlag("pick");

    // No invoke calls should have been made (flag didn't change)
    expect(spy).not.toHaveBeenCalled();
    expect(useProjectStore.getState().undoStack).toHaveLength(0);
  });

  test("ungrouped flag: undo entry has no batch", async () => {
    setupMockIpc({});

    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const images = [img1];
    const displayItems = computeDisplayItems(images, "triage", []);

    useProjectStore.setState({
      images,
      groups: [],
      displayItems,
      currentView: "triage",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlag("pick");

    const entry = useProjectStore.getState().undoStack[0];
    expect(entry.batch).toBeUndefined();
    expect(entry.imageId).toBe(1);
    expect(entry.oldValue).toBe("unreviewed");
    expect(entry.newValue).toBe("pick");
  });

  test("setFlagNoAutoReject on an already-picked image is a no-op", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);

    const img1 = makeImage({ id: 1, flag: "pick" });
    const img2 = makeImage({ id: 2, flag: "unreviewed" });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
    ]);
    const images = [img1, img2];
    const groups = [group];

    useProjectStore.setState({
      images,
      groups,
      displayItems: computeDisplayItems(images, "select", groups),
      currentView: "select",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlagNoAutoReject("pick");

    expect(spy).not.toHaveBeenCalled();
    expect(useProjectStore.getState().undoStack).toHaveLength(0);
    expect(useProjectStore.getState().images.find((i) => i.id === 2)!.flag).toBe("unreviewed");
  });

  test("setFlag clears redoStack", async () => {
    setupMockIpc({});

    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const images = [img1];
    const displayItems = computeDisplayItems(images, "triage", []);

    const fakeRedoEntry = {
      imageId: 99,
      field: "flag" as const,
      oldValue: "pick",
      newValue: "reject",
    };

    useProjectStore.setState({
      images,
      groups: [],
      displayItems,
      currentView: "triage",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [fakeRedoEntry],
    });

    await useProjectStore.getState().setFlag("pick");

    expect(useProjectStore.getState().redoStack).toHaveLength(0);
  });

  test("triage drill-down auto-exits when the last member is flagged", async () => {
    setupMockIpc({});

    // Two groups: user drills into groupA and picks its last unreviewed
    // frame. groupB still has unreviewed members. Previously the display
    // would collapse to [] (the drill-down path filtered the final
    // photo) and CullPage would render "Triage complete" even though
    // groupB is untouched. Regression guard: after the final pick,
    // activeInnerGroupId clears and displayItems falls back to the
    // outer list with groupB's cover visible.
    const a1 = makeImage({ id: 1, flag: "pick" });
    const a2 = makeImage({ id: 2, flag: "reject" });
    const a3 = makeImage({ id: 3, flag: "unreviewed" });
    const b1 = makeImage({ id: 4, flag: "unreviewed" });
    const b2 = makeImage({ id: 5, flag: "unreviewed" });
    const groupA = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);
    const groupB = makeGroup([
      { photoId: 4, isCover: true },
      { photoId: 5 },
    ]);

    const images = [a1, a2, a3, b1, b2];
    const groups = [groupA, groupB];

    // Emulate "drilled into groupA, focused on its last unreviewed
    // frame id=3." Setup displayItems as the drill-down list of one
    // photo so setFlag targets it.
    useProjectStore.setState({
      images,
      groups,
      displayItems: [{ imageIndex: 2, image: a3, groupId: groupA.id }],
      activeInnerGroupId: groupA.id,
      currentView: "triage",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
      showReviewed: false,
    });

    await useProjectStore.getState().setFlag("pick");

    const state = useProjectStore.getState();
    expect(state.activeInnerGroupId).toBeNull();
    // Outer triage: groupA now fully reviewed (no cover), groupB cover
    // still visible. So exactly one item remains.
    expect(state.displayItems.length).toBeGreaterThan(0);
    expect(state.displayItems.some((d) => d.groupId === groupB.id)).toBe(true);
    expect(state.displayItems.every((d) => d.groupId !== groupA.id)).toBe(true);
  });
});
