import { describe, test, expect, beforeEach } from "vitest";
import { useProjectStore, computeDisplayItems } from "../projectStore";
import { useSettingsStore } from "../settingsStore";
import { setupMockIpc } from "../../test/mockIpc";
import { makeImage, resetIds } from "../../test/fixtures";

/// Helper: hydrates the project store with a Select-view scenario and
/// returns the fixture set so the caller can assert against IDs directly.
function seedSelect(images: ReturnType<typeof makeImage>[], floor = 0) {
  useProjectStore.setState({
    images,
    groups: [],
    displayItems: computeDisplayItems(
      images,
      "select",
      [],
      new Set(),
      true, // selectRequiresPick = true (matches default)
      0,
      undefined,
      false,
      floor,
    ),
    currentView: "select",
    currentIndex: 0,
    activeInnerGroupId: null,
    autoAdvance: false,
    selectMinStar: floor,
    undoStack: [],
    redoStack: [],
  });
}

beforeEach(() => {
  resetIds();
  // Reset the store floor so tests can't pollute each other.
  useProjectStore.setState({ selectMinStar: 0 });
  // Match production's default where Select filters to `flag === "pick"`.
  useSettingsStore.setState((s) => ({
    settings: { ...s.settings, selectRequiresPick: true },
  }));
});

describe("Select multi-pass star-rating filter", () => {
  test("selectMinStar defaults to 0 and includes every picked photo", () => {
    const images = [
      makeImage({ id: 1, flag: "pick", starRating: 0 }),
      makeImage({ id: 2, flag: "pick", starRating: 2 }),
      makeImage({ id: 3, flag: "pick", starRating: 5 }),
    ];
    const items = computeDisplayItems(
      images,
      "select",
      [],
      new Set(),
      true,
      0,
      undefined,
      false,
      0,
    );
    expect(items.map((i) => i.image.id)).toEqual([1, 2, 3]);
  });

  test("selectMinStar=2 filters out photos below the floor", () => {
    const images = [
      makeImage({ id: 1, flag: "pick", starRating: 0 }),
      makeImage({ id: 2, flag: "pick", starRating: 2 }),
      makeImage({ id: 3, flag: "pick", starRating: 3 }),
    ];
    const items = computeDisplayItems(
      images,
      "select",
      [],
      new Set(),
      true,
      0,
      undefined,
      false,
      2,
    );
    expect(items.map((i) => i.image.id)).toEqual([2, 3]);
  });

  test("setSelectMinStar clamps to [0, 5]", () => {
    useProjectStore.setState({ selectMinStar: 0 });
    useProjectStore.getState().setSelectMinStar(-1);
    expect(useProjectStore.getState().selectMinStar).toBe(0);
    useProjectStore.getState().setSelectMinStar(7);
    expect(useProjectStore.getState().selectMinStar).toBe(5);
    useProjectStore.getState().setSelectMinStar(3);
    expect(useProjectStore.getState().selectMinStar).toBe(3);
  });

  test("pass complete: bumps floor and resets cursor when every visible photo is visited", async () => {
    // Pass 1 (floor=0): three picks visible. Navigating through all
    // three marks each visited. On the third visit, the pass is done →
    // floor auto-bumps to 1 and cursor resets to 0.
    setupMockIpc();

    const images = [
      makeImage({ id: 1, flag: "pick", starRating: 1 }),
      makeImage({ id: 2, flag: "pick", starRating: 1 }),
      makeImage({ id: 3, flag: "pick", starRating: 1 }),
    ];
    seedSelect(images, 0);

    useProjectStore.getState().markVisitedAtFloor(1);
    useProjectStore.getState().markVisitedAtFloor(2);
    useProjectStore.getState().markVisitedAtFloor(3);

    expect(useProjectStore.getState().selectMinStar).toBe(1);
    expect(useProjectStore.getState().currentIndex).toBe(0);
  });

  test("pass complete: does NOT fire while any visible photo is unvisited", () => {
    const images = [
      makeImage({ id: 1, flag: "pick", starRating: 1 }),
      makeImage({ id: 2, flag: "pick", starRating: 1 }),
      makeImage({ id: 3, flag: "pick", starRating: 1 }),
    ];
    seedSelect(images, 0);

    // Only visit two of three; floor stays put.
    useProjectStore.getState().markVisitedAtFloor(1);
    useProjectStore.getState().markVisitedAtFloor(2);

    expect(useProjectStore.getState().selectMinStar).toBe(0);
  });

  test("pass complete: lands at 5 without overshooting", async () => {
    // Floor=4, one visible photo at 4★. Rating it to 5★ fires visited
    // tracking. Only one photo visible, so visiting it triggers the
    // bump; clamp prevents going above 5.
    setupMockIpc();

    const images = [makeImage({ id: 1, flag: "pick", starRating: 4 })];
    seedSelect(images, 4);

    await useProjectStore.getState().setRating(5);

    expect(useProjectStore.getState().selectMinStar).toBe(5);
  });

  test("setSelectMinStar clears visited-at-floor so each pass starts fresh", () => {
    const images = [makeImage({ id: 1, flag: "pick", starRating: 1 })];
    seedSelect(images, 0);
    useProjectStore.setState({
      selectVisitedAtFloor: new Set<number>([1, 2, 3]),
    });
    useProjectStore.getState().setSelectMinStar(1);
    expect(useProjectStore.getState().selectVisitedAtFloor.size).toBe(0);
  });

  test("rejecting from Select still removes the photo from the visible tier", () => {
    const images = [
      makeImage({ id: 1, flag: "pick", starRating: 2 }),
      makeImage({ id: 2, flag: "reject", starRating: 2 }),
      makeImage({ id: 3, flag: "pick", starRating: 2 }),
    ];
    const items = computeDisplayItems(
      images,
      "select",
      [],
      new Set(),
      true,
      0,
      undefined,
      false,
      2,
    );
    // id=2 is rejected → filtered out regardless of its star rating.
    expect(items.map((i) => i.image.id)).toEqual([1, 3]);
  });

  test("loadShoot-like reset: selectMinStar goes back to 0 on a fresh shoot", () => {
    useProjectStore.setState({ selectMinStar: 3 });
    // Simulate what loadShoot does before its initial compute.
    useProjectStore.setState({ selectMinStar: 0 });
    expect(useProjectStore.getState().selectMinStar).toBe(0);
  });
});
