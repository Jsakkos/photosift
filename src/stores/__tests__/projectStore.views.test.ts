import { useProjectStore } from "../projectStore";
import { computeDisplayItems } from "../projectStore";
import { useSettingsStore } from "../settingsStore";
import { setupMockIpc } from "../../test/mockIpc";
import { makeImage, makeShoot, resetIds } from "../../test/fixtures";

beforeEach(() => {
  resetIds();
  useSettingsStore.setState({
    settings: {
      nearDupThreshold: 4,
      relatedThreshold: 12,
      triageExpandGroups: false,
      selectRequiresPick: false,
      routeMinStar: 0,
      libraryRoot: null,
    },
  });
});

describe("setView — view switching and cursor management", () => {
  test("setView saves cursor and restores for new view", async () => {
    const images = Array.from({ length: 10 }, () => makeImage());
    const shoot = makeShoot({ photoCount: 10 });
    const triageItems = computeDisplayItems(images, "triage", []);

    const calls: { cmd: string; args: unknown }[] = [];
    setupMockIpc(
      {
        get_view_cursor: null, // select has no saved cursor
      },
      (cmd, args) => calls.push({ cmd, args }),
    );

    // Set up state as if we loaded a shoot and navigated to index 5
    useProjectStore.setState({
      currentShoot: shoot,
      images,
      groups: [],
      currentView: "triage",
      currentIndex: 5,
      displayItems: triageItems,
    });

    await useProjectStore.getState().setView("select");

    // set_view_cursor should have been called to save triage position
    const saveCalls = calls.filter((c) => c.cmd === "set_view_cursor");
    expect(saveCalls).toHaveLength(1);
    expect(saveCalls[0].args).toEqual({
      shootId: shoot.id,
      viewName: "triage",
      photoId: images[5].id,
    });

    // get_view_cursor should have been called for the target view
    const getCalls = calls.filter((c) => c.cmd === "get_view_cursor");
    expect(getCalls).toHaveLength(1);
    expect((getCalls[0].args as Record<string, unknown>).viewName).toBe(
      "select",
    );

    const state = useProjectStore.getState();
    expect(state.currentView).toBe("select");
    expect(state.displayItems.length).toBeGreaterThan(0);
  });

  test("view roundtrip preserves positions", async () => {
    const images = Array.from({ length: 10 }, () => makeImage());
    const shoot = makeShoot({ photoCount: 10 });

    // Start in triage at index 3
    useProjectStore.setState({
      currentShoot: shoot,
      images,
      groups: [],
      currentView: "triage",
      currentIndex: 3,
      displayItems: computeDisplayItems(images, "triage", []),
    });

    // The image at triage index 3
    const triageImageId = images[3].id;

    // Switch to select — mock returns null cursor for select
    setupMockIpc({ get_view_cursor: null });
    await useProjectStore.getState().setView("select");
    expect(useProjectStore.getState().currentView).toBe("select");

    // Switch to route — mock returns null cursor for route
    setupMockIpc({ get_view_cursor: null });
    await useProjectStore.getState().setView("route");
    expect(useProjectStore.getState().currentView).toBe("route");

    // Return to triage — mock returns the saved cursor (triageImageId)
    setupMockIpc({ get_view_cursor: triageImageId });
    await useProjectStore.getState().setView("triage");

    const state = useProjectStore.getState();
    expect(state.currentView).toBe("triage");
    // The store finds the image by ID in displayItems
    expect(state.displayItems[state.currentIndex].image.id).toBe(
      triageImageId,
    );
    expect(state.currentIndex).toBe(3);
  });

  test("route view with 0 picks shows empty displayItems", async () => {
    // All images are unreviewed — none qualify for route (pick + unrouted)
    const images = Array.from({ length: 5 }, () =>
      makeImage({ flag: "unreviewed" }),
    );
    const shoot = makeShoot({ photoCount: 5 });

    useProjectStore.setState({
      currentShoot: shoot,
      images,
      groups: [],
      currentView: "triage",
      currentIndex: 0,
      displayItems: computeDisplayItems(images, "triage", []),
    });

    setupMockIpc({ get_view_cursor: null });
    await useProjectStore.getState().setView("route");

    const state = useProjectStore.getState();
    expect(state.currentView).toBe("route");
    expect(state.displayItems).toHaveLength(0);
  });

  test("setView is a no-op when currentShoot is null", async () => {
    useProjectStore.setState({ currentShoot: null, currentView: "triage" });

    setupMockIpc();
    await useProjectStore.getState().setView("select");

    expect(useProjectStore.getState().currentView).toBe("triage");
  });

  test("setView clamps to index 0 when saved cursor not found in displayItems", async () => {
    const images = Array.from({ length: 5 }, () => makeImage());
    const shoot = makeShoot();

    useProjectStore.setState({
      currentShoot: shoot,
      images,
      groups: [],
      currentView: "triage",
      currentIndex: 0,
      displayItems: computeDisplayItems(images, "triage", []),
    });

    // Return a cursor ID that doesn't exist in images
    setupMockIpc({ get_view_cursor: 9999 });
    await useProjectStore.getState().setView("select");

    expect(useProjectStore.getState().currentIndex).toBe(0);
  });
});
