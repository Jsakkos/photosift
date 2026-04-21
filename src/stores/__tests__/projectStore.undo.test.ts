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

describe("undo", () => {
  test("undo single flag revert", async () => {
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

    // Flag the image
    await useProjectStore.getState().setFlag("pick");
    expect(useProjectStore.getState().images[0].flag).toBe("pick");
    expect(useProjectStore.getState().undoStack).toHaveLength(1);

    // Undo
    await useProjectStore.getState().undo();

    const state = useProjectStore.getState();
    expect(state.images[0].flag).toBe("unreviewed");
    expect(state.undoStack).toHaveLength(0);
    expect(state.redoStack).toHaveLength(1);
    expect(state.redoStack[0].oldValue).toBe("unreviewed");
    expect(state.redoStack[0].newValue).toBe("pick");
  });

  test("undo Select auto-reject reverts the whole group batch", async () => {
    // Select's P auto-rejects siblings (pick-in-group cascades). Undoing
    // that one action must restore every photo touched by the cascade,
    // not just the primary pick. (Triage no longer has a bulk branch —
    // each photo is reviewed individually via auto-drill.)
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

    await useProjectStore.getState().setFlag("pick");
    expect(useProjectStore.getState().images.find((i) => i.id === 1)!.flag).toBe("pick");
    expect(useProjectStore.getState().images.find((i) => i.id === 2)!.flag).toBe("reject");
    expect(useProjectStore.getState().images.find((i) => i.id === 3)!.flag).toBe("reject");

    await useProjectStore.getState().undo();

    const state = useProjectStore.getState();
    expect(state.images.find((i) => i.id === 1)!.flag).toBe("unreviewed");
    expect(state.images.find((i) => i.id === 2)!.flag).toBe("unreviewed");
    expect(state.images.find((i) => i.id === 3)!.flag).toBe("unreviewed");

    expect(state.undoStack).toHaveLength(0);
    expect(state.redoStack).toHaveLength(1);
    expect(state.redoStack[0].batch).toHaveLength(3);
  });

  test("redo after Select-pick undo re-applies the cascade", async () => {
    setupMockIpc({});

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
      displayItems: computeDisplayItems([img1, img2, img3], "select", [group]),
      currentView: "select",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().setFlag("pick");
    await useProjectStore.getState().undo();
    await useProjectStore.getState().redo();

    const state = useProjectStore.getState();
    expect(state.images.find((i) => i.id === 1)!.flag).toBe("pick");
    expect(state.images.find((i) => i.id === 2)!.flag).toBe("reject");
    expect(state.images.find((i) => i.id === 3)!.flag).toBe("reject");
    expect(state.undoStack).toHaveLength(1);
    expect(state.redoStack).toHaveLength(0);
  });

  test("undo of comparison quickPick restores BOTH images", async () => {
    setupMockIpc({});

    const img1 = makeImage({ id: 1, flag: "unreviewed" });
    const img2 = makeImage({ id: 2, flag: "unreviewed" });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
    ]);

    useProjectStore.setState({
      images: [img1, img2],
      groups: [group],
      displayItems: computeDisplayItems([img1, img2], "select", [group]),
      currentView: "select",
      currentIndex: 0,
      comparisonPinnedId: 1,
      comparisonCyclingId: 2,
      comparisonGroupMembers: [1, 2],
      viewMode: "comparison",
      undoStack: [],
      redoStack: [],
    });

    await useProjectStore.getState().comparisonQuickPick("left");
    expect(useProjectStore.getState().images.find((i) => i.id === 1)!.flag).toBe("pick");
    expect(useProjectStore.getState().images.find((i) => i.id === 2)!.flag).toBe("reject");

    await useProjectStore.getState().undo();

    const state = useProjectStore.getState();
    expect(state.images.find((i) => i.id === 1)!.flag).toBe("unreviewed");
    expect(state.images.find((i) => i.id === 2)!.flag).toBe("unreviewed");
  });

  test("undo stack cap: each batch counts as one entry", async () => {
    setupMockIpc({});

    // 60 ungrouped single-flag actions should leave at most 50 entries.
    const images = Array.from({ length: 60 }, (_, i) =>
      makeImage({ id: i + 1, flag: "unreviewed" }),
    );
    useProjectStore.setState({
      images,
      groups: [],
      displayItems: computeDisplayItems(images, "triage", []),
      currentView: "triage",
      currentIndex: 0,
      autoAdvance: false,
      undoStack: [],
      redoStack: [],
    });

    for (let i = 0; i < 60; i++) {
      useProjectStore.setState({ currentIndex: i });
      await useProjectStore.getState().setFlag("pick");
    }

    // Cap keeps the 50 most recent entries (slice(-49) + the new one = 50).
    expect(useProjectStore.getState().undoStack.length).toBeLessThanOrEqual(50);
    expect(useProjectStore.getState().undoStack.length).toBeGreaterThan(0);
  });

  test("redo re-applies the flag", async () => {
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

    // Flag, undo, then redo
    await useProjectStore.getState().setFlag("reject");
    expect(useProjectStore.getState().images[0].flag).toBe("reject");

    await useProjectStore.getState().undo();
    expect(useProjectStore.getState().images[0].flag).toBe("unreviewed");

    await useProjectStore.getState().redo();

    const state = useProjectStore.getState();
    expect(state.images[0].flag).toBe("reject");
    // After redo, entry moves back to undoStack
    expect(state.undoStack).toHaveLength(1);
    expect(state.redoStack).toHaveLength(0);
  });

  test("empty undo stack is a no-op", async () => {
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

    // Undo with nothing on the stack
    await useProjectStore.getState().undo();

    const state = useProjectStore.getState();
    expect(state.images[0].flag).toBe("unreviewed");
    expect(state.undoStack).toHaveLength(0);
    expect(state.redoStack).toHaveLength(0);
    expect(state.currentIndex).toBe(0);
  });

  test("empty redo stack is a no-op", async () => {
    setupMockIpc({});

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

    await useProjectStore.getState().redo();

    const state = useProjectStore.getState();
    expect(state.images[0].flag).toBe("pick");
    expect(state.undoStack).toHaveLength(0);
    expect(state.redoStack).toHaveLength(0);
  });
});
