import { useProjectStore } from "../projectStore";
import { computeDisplayItems } from "../projectStore";
import { useSettingsStore } from "../settingsStore";
import { setupMockIpc } from "../../test/mockIpc";
import { makeImage, makeShoot, resetIds } from "../../test/fixtures";

beforeEach(() => {
  resetIds();
  setupMockIpc();
  // Disable the new Select/Route gating settings so these legacy tests exercise
  // the underlying view mechanics, not the gates themselves.
  useSettingsStore.setState({
    settings: {
      nearDupThreshold: 4,
      relatedThreshold: 12,
      groupTimeWindowS: 60,
      selectRequiresPick: false,
      routeMinStar: 0,
      libraryRoot: null,
      enableAiOnImport: true,
      hideSoftThreshold: 30,
      eyeOpenConfidence: 0.7,
    },
  });
});

/**
 * Helper: set up state in route view with picked+unrouted images.
 */
function setupRouteView(
  images: ReturnType<typeof makeImage>[],
  cursorIndex = 0,
) {
  const displayItems = computeDisplayItems(images, "route", []);
  useProjectStore.setState({
    currentShoot: makeShoot(),
    images,
    groups: [],
    currentView: "route",
    viewMode: "sequential",
    currentIndex: cursorIndex,
    displayItems,
  });
  return displayItems;
}

describe("setDestination", () => {
  test("updates image destination optimistically", async () => {
    const img1 = makeImage({ flag: "pick", destination: "unrouted" });
    const img2 = makeImage({ flag: "pick", destination: "unrouted" });

    setupRouteView([img1, img2], 0);

    await useProjectStore.getState().setDestination("edit");

    const state = useProjectStore.getState();
    const updated = state.images.find((i) => i.id === img1.id)!;
    expect(updated.destination).toBe("edit");
  });

  test("calls invoke with correct args", async () => {
    const img1 = makeImage({ flag: "pick", destination: "unrouted" });
    const calls: { cmd: string; args: unknown }[] = [];
    setupMockIpc({}, (cmd, args) => calls.push({ cmd, args }));

    setupRouteView([img1], 0);

    await useProjectStore.getState().setDestination("edit");

    const destCalls = calls.filter((c) => c.cmd === "set_destination");
    expect(destCalls).toHaveLength(1);
    expect(destCalls[0].args).toEqual({
      photoId: img1.id,
      destination: "edit",
    });
  });

  test("item leaves route displayItems after routing", async () => {
    const img1 = makeImage({ flag: "pick", destination: "unrouted" });
    const img2 = makeImage({ flag: "pick", destination: "unrouted" });
    const img3 = makeImage({ flag: "pick", destination: "unrouted" });

    setupRouteView([img1, img2, img3], 0);
    expect(useProjectStore.getState().displayItems).toHaveLength(3);

    await useProjectStore.getState().setDestination("edit");

    const state = useProjectStore.getState();
    // img1 is now destination="edit", no longer pick+unrouted
    expect(state.displayItems).toHaveLength(2);
    expect(state.displayItems.map((d) => d.image.id)).toEqual([
      img2.id,
      img3.id,
    ]);
  });

  test("index clamps when current item is routed at end of list", async () => {
    const img1 = makeImage({ flag: "pick", destination: "unrouted" });
    const img2 = makeImage({ flag: "pick", destination: "unrouted" });

    setupRouteView([img1, img2], 1);

    // Route the last item (index 1)
    await useProjectStore.getState().setDestination("publish_direct");

    const state = useProjectStore.getState();
    // Only img1 remains, index should clamp to 0
    expect(state.displayItems).toHaveLength(1);
    expect(state.currentIndex).toBe(0);
  });

  test("no-op when destination is the same", async () => {
    const img1 = makeImage({ flag: "pick", destination: "edit" });
    const calls: { cmd: string; args: unknown }[] = [];
    setupMockIpc({}, (cmd, args) => calls.push({ cmd, args }));

    // Manually place in route-like state (even though edit won't show in route)
    const displayItems = computeDisplayItems([img1], "select", []);
    useProjectStore.setState({
      currentShoot: makeShoot(),
      images: [img1],
      groups: [],
      currentView: "select",
      currentIndex: 0,
      displayItems,
    });

    await useProjectStore.getState().setDestination("edit");

    const destCalls = calls.filter((c) => c.cmd === "set_destination");
    expect(destCalls).toHaveLength(0);
  });

  test("adds undo entry on destination change", async () => {
    const img1 = makeImage({ flag: "pick", destination: "unrouted" });

    setupRouteView([img1], 0);
    expect(useProjectStore.getState().undoStack).toHaveLength(0);

    await useProjectStore.getState().setDestination("edit");

    const undoStack = useProjectStore.getState().undoStack;
    expect(undoStack).toHaveLength(1);
    expect(undoStack[0]).toMatchObject({
      imageId: img1.id,
      field: "destination",
      oldValue: "unrouted",
      newValue: "edit",
    });
  });

  test("routing all items leaves displayItems empty", async () => {
    const img1 = makeImage({ flag: "pick", destination: "unrouted" });

    setupRouteView([img1], 0);

    await useProjectStore.getState().setDestination("edit");

    const state = useProjectStore.getState();
    expect(state.displayItems).toHaveLength(0);
  });
});
