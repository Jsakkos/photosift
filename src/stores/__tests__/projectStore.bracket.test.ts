import { describe, test, expect, beforeEach, vi } from "vitest";
import { useProjectStore, computeDisplayItems } from "../projectStore";
import { setupMockIpc } from "../../test/mockIpc";
import { makeImage, makeGroup, makeShoot, resetIds } from "../../test/fixtures";

beforeEach(() => {
  resetIds();
  setupMockIpc();
});

function seedSelectWithGroup(
  images: ReturnType<typeof makeImage>[],
  group: ReturnType<typeof makeGroup>,
) {
  useProjectStore.setState({
    images,
    groups: [group],
    displayItems: computeDisplayItems(images, "select", [group]),
    currentView: "select",
    currentIndex: 0,
    autoAdvance: false,
    selectMinStar: 0,
    selectBracket: null,
    selectBracketSuppressedForGroup: null,
    selectVisitedAtFloor: new Set<number>(),
    undoStack: [],
    redoStack: [],
  });
}

describe("enterBracket", () => {
  test("seeds a bracket from the current group's reviewable members", () => {
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 80 });
    const img3 = makeImage({ id: 3, flag: "pick", starRating: 1, qualityScore: 70 });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);
    seedSelectWithGroup([img1, img2, img3], group);

    useProjectStore.getState().enterBracket();

    const state = useProjectStore.getState();
    expect(state.selectBracket).not.toBeNull();
    expect(state.selectBracket!.groupId).toBe(group.id);
    expect(state.selectBracket!.seedOrder).toEqual([1, 2, 3]);
  });

  test("no-op when the current item has no group", () => {
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1 });
    useProjectStore.setState({
      images: [img1],
      groups: [],
      displayItems: computeDisplayItems([img1], "select", []),
      currentView: "select",
      currentIndex: 0,
      selectBracket: null,
      selectBracketSuppressedForGroup: null,
      selectVisitedAtFloor: new Set<number>(),
    });

    useProjectStore.getState().enterBracket();

    expect(useProjectStore.getState().selectBracket).toBeNull();
  });

  test("explicit enterBracket (via Tab) clears the suppressedForGroup lock", () => {
    // The SelectShell auto-enter effect respects suppression, but a
    // direct enterBracket call (user hitting Tab) overrides it. This
    // lets users toggle 2-up back on without leaving the group first.
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1 });
    const group = makeGroup([{ photoId: 1 }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2], group);
    useProjectStore.setState({ selectBracketSuppressedForGroup: group.id });

    useProjectStore.getState().enterBracket();

    const state = useProjectStore.getState();
    expect(state.selectBracket).not.toBeNull();
    expect(state.selectBracketSuppressedForGroup).toBeNull();
  });
});

describe("bracketDecision", () => {
  test("L bumps left photo by 1 star and advances to next pair", async () => {
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 100 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 90 });
    const img3 = makeImage({ id: 3, flag: "pick", starRating: 1, qualityScore: 80 });
    const img4 = makeImage({ id: 4, flag: "pick", starRating: 1, qualityScore: 70 });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
      { photoId: 4 },
    ]);
    seedSelectWithGroup([img1, img2, img3, img4], group);
    useProjectStore.getState().enterBracket();

    await useProjectStore.getState().bracketDecision("L");

    const state = useProjectStore.getState();
    expect(state.images.find((i) => i.id === 1)!.starRating).toBe(2);
    expect(state.images.find((i) => i.id === 2)!.starRating).toBe(1);
    expect(state.selectBracket).not.toBeNull();
    expect(state.selectBracket!.currentPairIndex).toBe(1);
  });

  test("Both bumps both photos by 1 star", async () => {
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 80 });
    const group = makeGroup([{ photoId: 1, isCover: true }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2], group);
    useProjectStore.getState().enterBracket();

    await useProjectStore.getState().bracketDecision("both");

    const state = useProjectStore.getState();
    expect(state.images.find((i) => i.id === 1)!.starRating).toBe(2);
    expect(state.images.find((i) => i.id === 2)!.starRating).toBe(2);
  });

  test("bracket completion drops bracket state and triggers pass-complete", async () => {
    // Two-member group at the bottom floor; the single bracket decision
    // marks both members visited. Since they're the only displayItems,
    // the floor bumps from 0 → 1 as the pass-complete side effect.
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 80 });
    const group = makeGroup([{ photoId: 1, isCover: true }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2], group);
    useProjectStore.getState().enterBracket();

    await useProjectStore.getState().bracketDecision("L");

    const state = useProjectStore.getState();
    expect(state.selectBracket).toBeNull();
    expect(state.selectMinStar).toBe(1);
    // After the floor bumps, the visited set is reset for the new tier.
    expect(state.selectVisitedAtFloor.size).toBe(0);
  });

  test("bracket completion without pass-bump leaves visited set populated", async () => {
    // Three-photo shoot: a 2-member group plus a lone ungrouped photo.
    // The bracket only marks the group members visited; the solo photo
    // is untouched so the pass does NOT auto-complete.
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 80 });
    const img3 = makeImage({ id: 3, flag: "pick", starRating: 1 });
    const group = makeGroup([{ photoId: 1, isCover: true }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2, img3], group);
    useProjectStore.getState().enterBracket();

    await useProjectStore.getState().bracketDecision("L");

    const state = useProjectStore.getState();
    expect(state.selectBracket).toBeNull();
    expect(state.selectMinStar).toBe(0); // no bump yet
    expect(state.selectVisitedAtFloor.has(1)).toBe(true);
    expect(state.selectVisitedAtFloor.has(2)).toBe(true);
    expect(state.selectVisitedAtFloor.has(3)).toBe(false);
  });

  test("star promotion clamps at 5", async () => {
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 5, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 5, qualityScore: 80 });
    const group = makeGroup([{ photoId: 1, isCover: true }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2], group);
    useProjectStore.getState().enterBracket();

    await useProjectStore.getState().bracketDecision("both");

    const state = useProjectStore.getState();
    expect(state.images.find((i) => i.id === 1)!.starRating).toBe(5);
    expect(state.images.find((i) => i.id === 2)!.starRating).toBe(5);
  });
});

describe("exitBracket", () => {
  test("locks the current group so auto-enter skips it", () => {
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1 });
    const group = makeGroup([{ photoId: 1, isCover: true }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2], group);
    useProjectStore.getState().enterBracket();
    expect(useProjectStore.getState().selectBracket).not.toBeNull();

    useProjectStore.getState().exitBracket();

    const state = useProjectStore.getState();
    expect(state.selectBracket).toBeNull();
    expect(state.selectBracketSuppressedForGroup).toBe(group.id);
  });
});

describe("enterBracket — Wave 1 filter gates (#22 + #16)", () => {
  test("respects active selectMinStar floor (#22)", () => {
    // 0★ photo paired against 2★ photos in a ★≥2 bracket is the
    // exact regression captured in issue #22.
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 0 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 2, qualityScore: 80 });
    const img3 = makeImage({ id: 3, flag: "pick", starRating: 2, qualityScore: 70 });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);
    seedSelectWithGroup([img1, img2, img3], group);
    useProjectStore.setState({ selectMinStar: 2 });

    useProjectStore.getState().enterBracket();

    const state = useProjectStore.getState();
    expect(state.selectBracket).not.toBeNull();
    // 0★ photo (id=1) must NOT be in the bracket
    expect(state.selectBracket!.seedOrder).not.toContain(1);
    expect(state.selectBracket!.seedOrder.sort()).toEqual([2, 3]);
  });

  test("excludes routed-eligible picks (#16)", () => {
    // routeMinStar default is 3; picks at ★≥3 are 'ready to route'
    // and should not appear in subsequent Select brackets.
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 80 });
    const img3 = makeImage({ id: 3, flag: "pick", starRating: 4 });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);
    seedSelectWithGroup([img1, img2, img3], group);

    useProjectStore.getState().enterBracket();

    const state = useProjectStore.getState();
    expect(state.selectBracket).not.toBeNull();
    expect(state.selectBracket!.seedOrder).not.toContain(3);
    expect(state.selectBracket!.seedOrder.sort()).toEqual([1, 2]);
  });

  test("does not enter when fewer than 2 members survive both gates", () => {
    // floor=3, all photos at ★0 — bracket must not enter (members.length < 2)
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 0 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 0 });
    const img3 = makeImage({ id: 3, flag: "pick", starRating: 0 });
    const group = makeGroup([
      { photoId: 1, isCover: true },
      { photoId: 2 },
      { photoId: 3 },
    ]);
    seedSelectWithGroup([img1, img2, img3], group);
    useProjectStore.setState({ selectMinStar: 3 });

    useProjectStore.getState().enterBracket();

    expect(useProjectStore.getState().selectBracket).toBeNull();
  });
});

describe("bracketDecision — Review-tab persistence", () => {
  test("persists each decision via record_bracket_decision", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 80 });
    const group = makeGroup([{ photoId: 1, isCover: true }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2], group);
    useProjectStore.setState({ currentShoot: makeShoot({ id: 7 }) });
    useProjectStore.getState().enterBracket();

    await useProjectStore.getState().bracketDecision("L");

    const call = spy.mock.calls.find((c) => c[0] === "record_bracket_decision");
    expect(call).toBeDefined();
    const args = call![1] as {
      shootId: number;
      groupId: number;
      decision: string;
      leftPhotoId: number;
      rightPhotoId: number | null;
    };
    expect(args.shootId).toBe(7);
    expect(args.groupId).toBe(group.id);
    expect(args.decision).toBe("L");
    expect(args.leftPhotoId).toBe(1);
    expect(args.rightPhotoId).toBe(2);
  });

  test("undo of a tournament pick deletes its persisted bracket row", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);
    const img1 = makeImage({ id: 1, flag: "pick", starRating: 1, qualityScore: 90 });
    const img2 = makeImage({ id: 2, flag: "pick", starRating: 1, qualityScore: 80 });
    const group = makeGroup([{ photoId: 1, isCover: true }, { photoId: 2 }]);
    seedSelectWithGroup([img1, img2], group);
    useProjectStore.setState({ currentShoot: makeShoot({ id: 7 }) });
    useProjectStore.getState().enterBracket();
    await useProjectStore.getState().bracketDecision("L");

    await useProjectStore.getState().undo();

    expect(
      spy.mock.calls.find((c) => c[0] === "delete_bracket_decision"),
    ).toBeDefined();
  });
});
