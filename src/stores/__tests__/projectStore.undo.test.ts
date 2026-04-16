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

  test("undo group flag in triage only reverts cover image (known bug)", async () => {
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
    const displayItems = computeDisplayItems(images, "triage", groups);

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

    // Flag group cover -- all 3 members get flagged
    await useProjectStore.getState().setFlag("pick");
    expect(useProjectStore.getState().images.every((i) => i.flag === "pick")).toBe(true);

    // Undo -- the undo entry only stores imageId for the cover (id=1).
    // Only the cover image's flag is reverted. Siblings stay "pick".
    // This is a known limitation: batchSize is stored but undo only
    // reverts the single entry.imageId.
    await useProjectStore.getState().undo();

    const state = useProjectStore.getState();
    // Cover reverts to "unreviewed"
    expect(state.images.find((i) => i.id === 1)!.flag).toBe("unreviewed");
    // Siblings remain "pick" -- undo doesn't know about them
    expect(state.images.find((i) => i.id === 2)!.flag).toBe("pick");
    expect(state.images.find((i) => i.id === 3)!.flag).toBe("pick");

    expect(state.undoStack).toHaveLength(0);
    expect(state.redoStack).toHaveLength(1);
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
