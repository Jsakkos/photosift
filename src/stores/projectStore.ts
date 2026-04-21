import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  ImageEntry,
  ShootSummary,
  CullView,
  ViewMode,
  DisplayItem,
  Group,
} from "../types";
import { useSettingsStore } from "./settingsStore";
import { useAiStore } from "./aiStore";

function selectRequiresPick(): boolean {
  return useSettingsStore.getState().settings.selectRequiresPick ?? false;
}

function routeMinStar(): number {
  return useSettingsStore.getState().settings.routeMinStar ?? 0;
}

function hideSoftThreshold(): number {
  return useSettingsStore.getState().settings.hideSoftThreshold ?? 0;
}

/// Bundle the sort/filter/pick options that `computeDisplayItems` needs.
/// Centralized so call sites don't have to remember to keep
/// `useEyesInPick` / `useSmileInPick` gated on the provider kinds — if the
/// backend swaps to a real classifier, this helper picks it up automatically.
function currentAiOptions(
  sortByAi: "none" | "sharpness" | "faces",
): AiDisplayOptions {
  const { eyeProvider, mouthProvider } = useAiStore.getState();
  return {
    sortByAi,
    hideSoftThreshold: hideSoftThreshold(),
    useEyesInPick: eyeProvider === "onnx",
    useSmileInPick: mouthProvider === "onnx",
  };
}

export interface AiDisplayOptions {
  sortByAi: "none" | "sharpness" | "faces";
  hideSoftThreshold: number; // 0 disables
  /// When false, AI-pick scoring ignores the `eyes_open` term. Must be
  /// false while the eye classifier is `MockEyeProvider`.
  useEyesInPick: boolean;
  /// When false, AI-pick scoring ignores the smile term. Must be false
  /// while the mouth classifier is the mock/1-class/2-class stub — those
  /// return a constant 0.5 that would bias picks indiscriminately.
  useSmileInPick: boolean;
}

const DEFAULT_AI_OPTIONS: AiDisplayOptions = {
  sortByAi: "none",
  hideSoftThreshold: 0,
  useEyesInPick: false,
  useSmileInPick: false,
};

interface UndoEntry {
  imageId: number;
  field: "starRating" | "flag" | "destination";
  oldValue: string | number;
  newValue: string | number;
  batch?: {
    imageId: number;
    oldValue: string | number;
    newValue: string | number;
  }[];
}

function describeUndoRedoEntry(action: "Undo" | "Redo", entry: UndoEntry): string {
  const count = entry.batch ? entry.batch.length : 1;
  const fieldLabel =
    entry.field === "starRating" ? "rating"
    : entry.field === "flag" ? "flag"
    : "destination";
  const valueLabel = (v: string | number): string =>
    entry.field === "starRating" ? `${v}★` : String(v);
  // For batch ops, show the value being restored/reapplied (they're
  // all the same). For single ops, show both sides for clarity.
  const shown = action === "Undo" ? entry.oldValue : entry.newValue;
  const suffix = count > 1 ? ` · ${count} photos` : "";
  return `${action}: ${fieldLabel} → ${valueLabel(shown)}${suffix}`;
}

/// Wrapper around `computeDisplayItems` that applies the Narrative-
/// Select drill-down filter: when an inner group is active, the
/// visible (keyboard-navigable) set shrinks to just that group's
/// members. Outside a drill-down this is a no-op passthrough.
function computeDisplayItemsFiltered(
  images: ImageEntry[],
  currentView: CullView,
  groups: Group[],
  activeInnerGroupId: number | null,
  selectRequiresPickFilter: boolean,
  routeMinStarGate: number,
  aiOptions: AiDisplayOptions = DEFAULT_AI_OPTIONS,
): DisplayItem[] {
  if (activeInnerGroupId == null) {
    return computeDisplayItems(
      images,
      currentView,
      groups,
      new Set<number>(),
      selectRequiresPickFilter,
      routeMinStarGate,
      aiOptions,
    );
  }

  // Drilled in: enumerate the target group's members directly. We can't
  // reuse `computeDisplayItems` because its photo→group map is lossy —
  // the Rust two-tier clusterer emits BOTH a tight near_duplicate group
  // and a broader related group covering the same photos, so a single
  // photoId resolves to whichever group was registered last. Drilling
  // into the "losing" group would then return zero members. Walking the
  // target group's own `members` array avoids the ambiguity entirely.
  const group = groups.find((g) => g.id === activeInnerGroupId);
  if (!group) return [];

  const pick = aiPickForGroup(group, images, aiOptions.useEyesInPick, aiOptions.useSmileInPick);
  const result: DisplayItem[] = [];
  for (const m of group.members) {
    const imgIdx = images.findIndex((i) => i.id === m.photoId);
    if (imgIdx < 0) continue;
    const img = images[imgIdx];

    if (currentView === "triage") {
      if (img.flag === "reject") continue;
      if (img.flag !== "unreviewed" && img.id !== pick) continue;
    } else if (currentView === "select") {
      const passes = selectRequiresPickFilter
        ? img.flag === "pick"
        : img.flag !== "reject";
      if (!passes) continue;
    } else {
      // route view
      if (img.flag !== "pick" || img.destination !== "unrouted") continue;
      if (routeMinStarGate > 0 && img.starRating < routeMinStarGate) continue;
    }

    result.push({
      imageIndex: imgIdx,
      image: img,
      groupId: group.id,
      ...(pick === img.id ? { isAiPick: true } : {}),
    });
  }

  // Within-group ranking: best quality first so sequential navigation
  // (arrow keys, Space-to-next) cycles through the likely picks before
  // the filler shots. Unanalyzed photos (qualityScore == null) sort last
  // so they don't push real candidates down. AI pick is a tiebreaker
  // ahead of anything at the same score.
  result.sort((a, b) => {
    const aq = typeof a.image.qualityScore === "number" ? a.image.qualityScore : -Infinity;
    const bq = typeof b.image.qualityScore === "number" ? b.image.qualityScore : -Infinity;
    if (aq !== bq) return bq - aq;
    if (a.isAiPick && !b.isAiPick) return -1;
    if (b.isAiPick && !a.isAiPick) return 1;
    return a.imageIndex - b.imageIndex;
  });
  return result;
}

export function buildPhotoGroupMap(groups: Group[]): Map<number, Group> {
  const map = new Map<number, Group>();
  for (const g of groups) {
    for (const m of g.members) {
      map.set(m.photoId, g);
    }
  }
  return map;
}

export function getGroupCover(group: Group): number {
  const cover = group.members.find((m) => m.isCover);
  return cover ? cover.photoId : group.members[0].photoId;
}

/// Returns the id of the AI-recommended photo in the group, or null when
/// fewer than 2 members have been analyzed.
///
/// Scoring: `sharp * (1 + eyes_open_count) * (1 + 0.5 * smile_score)`
/// when both flags are on. Each factor contributes multiplicatively so the
/// signals compose cleanly; mock providers stay disabled because their
/// constant outputs would cancel out (or worse, uniformly bias).
/// Ties broken by lower id.
export function aiPickForGroup(
  group: Group,
  images: ImageEntry[],
  useEyes: boolean = false,
  useSmile: boolean = false,
): number | null {
  const analyzed = group.members
    .map((m) => images.find((i) => i.id === m.photoId))
    .filter((img): img is ImageEntry => !!img && img.aiAnalyzedAt != null);

  if (analyzed.length < 2) return null;

  const scoreOf = (img: ImageEntry): number => {
    let s = img.sharpnessScore ?? 0;
    if (useEyes) s *= 1 + (img.eyesOpenCount ?? 0);
    if (useSmile) s *= 1 + 0.5 * (img.maxSmileScore ?? 0);
    return s;
  };

  let bestId = analyzed[0].id;
  let bestScore = scoreOf(analyzed[0]);

  for (const img of analyzed.slice(1)) {
    const score = scoreOf(img);
    if (score > bestScore || (score === bestScore && img.id < bestId)) {
      bestId = img.id;
      bestScore = score;
    }
  }
  return bestId;
}

export function computeDisplayItems(
  images: ImageEntry[],
  currentView: CullView,
  groups: Group[],
  expandedGroupIds: Set<number> = new Set(),
  selectRequiresPickFilter: boolean = false,
  routeMinStarGate: number = 0,
  aiOptions: AiDisplayOptions = DEFAULT_AI_OPTIONS,
): DisplayItem[] {
  const items: DisplayItem[] = [];
  const photoGroupMap = buildPhotoGroupMap(groups);

  if (currentView === "triage") {
    // Pre-compute AI pick per group so we can keep the recommended member
    // visible in expanded display even after it's been flagged. Without
    // this, the `★ AI` badge loses its target the moment the user picks
    // or rejects the pick photo, since the default filter drops
    // non-unreviewed members.
    const triagePickCache = new Map<number, number | null>();
    const pickForGroup = (g: Group): number | null => {
      if (triagePickCache.has(g.id)) return triagePickCache.get(g.id)!;
      const p = aiPickForGroup(g, images, aiOptions.useEyesInPick, aiOptions.useSmileInPick);
      triagePickCache.set(g.id, p);
      return p;
    };

    const seenGroups = new Set<number>();
    for (let i = 0; i < images.length; i++) {
      const img = images[i];
      if (img.flag !== "unreviewed") continue;

      const group = photoGroupMap.get(img.id);
      if (group) {
        if (seenGroups.has(group.id)) continue;
        seenGroups.add(group.id);
        if (expandedGroupIds.has(group.id)) {
          // Drill-down: emit each unreviewed member inline with groupId set
          // so the filmstrip can draw the affiliation bar. The AI pick is
          // emitted even if flagged so the badge has a place to render.
          for (const member of group.members) {
            const mi = images.findIndex((im) => im.id === member.photoId);
            if (mi < 0) continue;
            const memImg = images[mi];
            const isPinnedPick = pickForGroup(group) === memImg.id;
            if (memImg.flag === "reject") continue;
            if (memImg.flag !== "unreviewed" && !isPinnedPick) continue;
            items.push({
              imageIndex: mi,
              image: memImg,
              groupId: group.id,
            });
          }
          continue;
        }
        const coverId = getGroupCover(group);
        const coverIdx = images.findIndex((im) => im.id === coverId);
        const coverImg = coverIdx >= 0 ? images[coverIdx] : img;
        const actualIdx = coverIdx >= 0 ? coverIdx : i;
        const unrevCount = group.members.filter((m) => {
          const mi = images.find((im) => im.id === m.photoId);
          return mi && mi.flag === "unreviewed";
        }).length;
        if (unrevCount === 0) continue;
        items.push({
          imageIndex: actualIdx,
          image: coverImg,
          groupId: group.id,
          isGroupCover: true,
          groupMemberCount: group.members.length,
        });
      } else {
        items.push({ imageIndex: i, image: img });
      }
    }
  } else if (currentView === "select") {
    const seenGroups = new Set<number>();
    const passesSelectGate = (img: ImageEntry): boolean =>
      selectRequiresPickFilter ? img.flag === "pick" : img.flag !== "reject";

    for (let i = 0; i < images.length; i++) {
      const img = images[i];
      if (!passesSelectGate(img)) continue;

      const group = photoGroupMap.get(img.id);
      if (group) {
        if (seenGroups.has(group.id)) continue;
        seenGroups.add(group.id);
        for (const member of group.members) {
          const memberIdx = images.findIndex((im) => im.id === member.photoId);
          if (memberIdx < 0) continue;
          const memberImg = images[memberIdx];
          if (!passesSelectGate(memberImg)) continue;
          items.push({
            imageIndex: memberIdx,
            image: memberImg,
            groupId: group.id,
          });
        }
      } else {
        items.push({ imageIndex: i, image: img });
      }
    }
  } else {
    for (let i = 0; i < images.length; i++) {
      const img = images[i];
      if (
        img.flag === "pick" &&
        img.destination === "unrouted" &&
        (routeMinStarGate === 0 || img.starRating >= routeMinStarGate)
      ) {
        items.push({ imageIndex: i, image: img });
      }
    }
  }

  // AI filter: hideSoft (Select + Route only). Nulls pass through so images
  // still being analyzed remain visible; the gate only evicts known-soft shots.
  let result = items;
  if (
    aiOptions.hideSoftThreshold > 0 &&
    (currentView === "select" || currentView === "route")
  ) {
    result = result.filter((it) => {
      const s = it.image.sharpnessScore;
      return s === null || s === undefined || s >= aiOptions.hideSoftThreshold;
    });
  }

  // AI sort: stable sort, nulls/undefineds to the end.
  if (aiOptions.sortByAi === "sharpness") {
    result = [...result].sort((a, b) => {
      const sa = a.image.sharpnessScore;
      const sb = b.image.sharpnessScore;
      const na = sa === null || sa === undefined;
      const nb = sb === null || sb === undefined;
      if (na && nb) return 0;
      if (na) return 1;
      if (nb) return -1;
      return (sb as number) - (sa as number);
    });
  } else if (aiOptions.sortByAi === "faces") {
    result = [...result].sort((a, b) => {
      const fa = a.image.faceCount;
      const fb = b.image.faceCount;
      const na = fa === null || fa === undefined;
      const nb = fb === null || fb === undefined;
      if (na && nb) return 0;
      if (na) return 1;
      if (nb) return -1;
      return (fb as number) - (fa as number);
    });
  }

  // AI pick derivation: for each group with ≥2 analyzed members, mark the
  // recommended photo. We memoize per groupId so the scoring runs once per
  // group regardless of how many members are emitted in the display list.
  const pickCache = new Map<number, number | null>();
  for (const it of result) {
    if (it.groupId === undefined) continue;
    if (!pickCache.has(it.groupId)) {
      const g = groups.find((gg) => gg.id === it.groupId);
      pickCache.set(
        it.groupId,
        g ? aiPickForGroup(g, images, aiOptions.useEyesInPick, aiOptions.useSmileInPick) : null,
      );
    }
    if (pickCache.get(it.groupId) === it.image.id) {
      it.isAiPick = true;
    }
  }

  return result;
}

interface ProjectState {
  currentShoot: ShootSummary | null;
  images: ImageEntry[];
  currentIndex: number;
  isLoading: boolean;
  loadError: string | null;
  showMetadata: boolean;
  showShortcutHints: boolean;
  autoAdvance: boolean;
  isZoomed: boolean;
  undoStack: UndoEntry[];
  redoStack: UndoEntry[];
  currentView: CullView;
  viewMode: ViewMode;
  groups: Group[];
  displayItems: DisplayItem[];
  activeInnerGroupId: number | null;
  lastFlagAction: { color: string; timestamp: number } | null;
  toast: { message: string; kind: "info" | "error"; timestamp: number } | null;

  loadShoot: (shootId: number) => Promise<void>;
  /// Narrative-Select-style drilldown. Pass a groupId to open the inner
  /// strip for that group; pass the same id to toggle closed; pass null
  /// to clear. Only one group can be active at a time.
  setActiveInnerGroup: (groupId: number | null) => void;
  setCurrentIndex: (index: number) => void;
  navigateNext: () => void;
  navigatePrev: () => void;
  setRating: (rating: number) => Promise<void>;
  setFlag: (flag: string) => Promise<void>;
  setDestination: (dest: string) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  toggleMetadata: () => void;
  toggleShortcutHints: () => void;
  aiPanelForced: boolean;
  toggleAiPanel: () => void;
  toggleAutoAdvance: () => void;
  toggleZoom: () => void;
  setView: (view: CullView) => Promise<void>;
  setViewMode: (mode: ViewMode) => void;
  advanceToNextUnreviewed: () => void;
  clearFlagFlash: () => void;
  setToast: (message: string, kind?: "info" | "error") => void;
  clearToast: () => void;
  currentImage: () => ImageEntry | null;
  setFlagNoAutoReject: (flag: string) => Promise<void>;
  setGroupCover: (groupId: number, photoId: number) => Promise<void>;
  acceptAiPick: () => Promise<void>;
  getGroupForCurrentItem: () => Group | null;
  comparisonPinnedId: number | null;
  comparisonCyclingId: number | null;
  comparisonGroupMembers: number[];
  enterComparison: () => void;
  exitComparison: () => void;
  cycleComparison: (direction: 1 | -1) => void;
  comparisonQuickPick: (side: "left" | "right") => Promise<void>;
  createGroupFromPhotos: (photoIds: number[]) => Promise<void>;
  ungroupPhotos: (photoIds: number[]) => Promise<void>;
  refreshDisplay: () => void;
  patchImageAiData: (photoId: number) => Promise<void>;
  /// Append a newly-imported photo to `images` without reloading the
  /// whole shoot. Called from the CullPage event subscription when an
  /// `import-photo-ready` event fires for the currently-loaded shoot.
  /// Idempotent — duplicates the existing image list check before fetching.
  appendImportedPhoto: (photoId: number) => Promise<void>;
  /// Refetch groups for the current shoot (called after clustering
  /// completes mid-import).
  refetchGroups: () => Promise<void>;
  sortByAi: "none" | "sharpness" | "faces";
  cycleSortByAi: () => void;
  heatmapOn: boolean;
  heatmapCache: Map<number, number[]>;
  toggleHeatmap: () => void;
  getHeatmapData: (photoId: number) => number[] | null;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  currentShoot: null,
  images: [],
  currentIndex: 0,
  isLoading: false,
  loadError: null,
  showMetadata: false,
  showShortcutHints: false,
  aiPanelForced: false,
  autoAdvance: true,
  isZoomed: false,
  undoStack: [],
  redoStack: [],
  currentView: "triage",
  viewMode: "sequential",
  groups: [],
  displayItems: [],
  activeInnerGroupId: null,
  lastFlagAction: null,
  toast: null,
  comparisonPinnedId: null,
  comparisonCyclingId: null,
  comparisonGroupMembers: [],
  sortByAi: "none" as const,
  heatmapOn: false,
  heatmapCache: new Map<number, number[]>(),

  currentImage: () => {
    const { displayItems, currentIndex } = get();
    return displayItems[currentIndex]?.image ?? null;
  },

  loadShoot: async (shootId: number) => {
    set({ isLoading: true, loadError: null });
    try {
      const shoot = await invoke<ShootSummary>("get_shoot", { shootId });
      const images = await invoke<ImageEntry[]>("get_image_list");

      const groups = await invoke<Group[]>("get_groups_for_shoot", {
        shootId,
      }).catch(() => [] as Group[]);

      // Resume in whichever view the user last opened for this shoot.
      // Falls back to triage for first-time opens so the standard flow
      // still kicks off on ingest.
      const resumeView: CullView = shoot.lastView ?? "triage";

      const cursor = await invoke<number | null>("get_view_cursor", {
        shootId,
        viewName: resumeView,
      }).catch(() => null);

      const displayItems = computeDisplayItemsFiltered(
        images,
        resumeView,
        groups,
          null,
        selectRequiresPick(),
        routeMinStar(),
        currentAiOptions("none"),
      );

      let startIndex = 0;
      if (cursor !== null) {
        const idx = displayItems.findIndex((d) => d.image.id === cursor);
        if (idx >= 0) startIndex = idx;
      }

      set({
        currentShoot: shoot,
        images,
        currentIndex: startIndex,
        isLoading: false,
        undoStack: [],
        redoStack: [],
        currentView: resumeView,
        viewMode: "sequential",
        groups,
        displayItems,
        activeInnerGroupId: null,
        lastFlagAction: null,
      });

      // Kick off the shoot's sharpness-percentile fetch so the face-panel
      // badge has the right 1-10 scale ready by the time the user opens a
      // photo. The fetch is cheap and the Rust side caches it.
      useAiStore.getState().fetchPercentiles(shoot.id).catch(() => {});
    } catch (e) {
      console.error("Failed to load shoot:", e);
      set({ isLoading: false, loadError: String(e) });
    }
  },

  setCurrentIndex: (index: number) => {
    const { displayItems } = get();
    if (index >= 0 && index < displayItems.length) {
      set({ currentIndex: index, isZoomed: false });
    }
  },

  navigateNext: () => {
    const { currentIndex, displayItems } = get();
    if (currentIndex < displayItems.length - 1) {
      set({ currentIndex: currentIndex + 1, isZoomed: false });
    }
  },

  navigatePrev: () => {
    const { currentIndex } = get();
    if (currentIndex > 0) {
      set({ currentIndex: currentIndex - 1, isZoomed: false });
    }
  },

  setRating: async (rating: number) => {
    const { displayItems, currentIndex, autoAdvance, undoStack, images } =
      get();
    const item = displayItems[currentIndex];
    if (!item) return;

    const image = item.image;
    const oldRating = image.starRating;
    if (oldRating === rating) return;

    const updatedImages = [...images];
    updatedImages[item.imageIndex] = { ...image, starRating: rating };
    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      get().currentView,
      get().groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      undoStack: [
        ...undoStack.slice(-49),
        {
          imageId: image.id,
          field: "starRating",
          oldValue: oldRating,
          newValue: rating,
        },
      ],
      redoStack: [],
    });

    if (autoAdvance && currentIndex < newDisplayItems.length - 1) {
      set({ currentIndex: currentIndex + 1, isZoomed: false });
    }

    try {
      await invoke("set_rating", { imageId: image.id, rating });
    } catch (e) {
      console.error("Failed to set rating:", e);
      get().setToast(`Rating save failed: ${e}`, "error");
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], starRating: oldRating };
        set({
          images: revertImages,
          displayItems: computeDisplayItemsFiltered(
            revertImages,
            get().currentView,
            get().groups,
                  get().activeInnerGroupId,
            selectRequiresPick(),
            routeMinStar(),
            currentAiOptions(get().sortByAi),
          ),
        });
      }
    }
  },

  setFlag: async (flag: string) => {
    const {
      displayItems,
      currentIndex,
      autoAdvance,
      undoStack,
      images,
      currentView,
      groups,
    } = get();
    const item = displayItems[currentIndex];
    if (!item) return;

    const image = item.image;
    const oldFlag = image.flag;
    if (oldFlag === flag) return;

    const updatedImages = [...images];
    const affectedIds: { id: number; oldFlag: string }[] = [];

    if (item.groupId && currentView === "triage" && item.isGroupCover) {
      const group = groups.find((g) => g.id === item.groupId);
      if (group) {
        for (const member of group.members) {
          const mi = updatedImages.findIndex((im) => im.id === member.photoId);
          if (mi >= 0) {
            affectedIds.push({ id: member.photoId, oldFlag: updatedImages[mi].flag });
            updatedImages[mi] = { ...updatedImages[mi], flag };
          }
        }
      }
    } else if (item.groupId && currentView === "select" && flag === "pick") {
      updatedImages[item.imageIndex] = { ...image, flag };
      affectedIds.push({ id: image.id, oldFlag });
      const group = groups.find((g) => g.id === item.groupId);
      if (group) {
        const siblingIds: number[] = [];
        for (const member of group.members) {
          if (member.photoId === image.id) continue;
          const mi = updatedImages.findIndex((im) => im.id === member.photoId);
          if (mi >= 0 && updatedImages[mi].flag !== "reject") {
            affectedIds.push({ id: member.photoId, oldFlag: updatedImages[mi].flag });
            updatedImages[mi] = { ...updatedImages[mi], flag: "reject" };
            siblingIds.push(member.photoId);
          }
        }
        if (siblingIds.length > 0) {
          invoke("bulk_set_flag", { photoIds: siblingIds, flag: "reject" }).catch(
            (err) => {
              get().setToast(`Group reject failed: ${err}`, "error");
            },
          );
        }
      }
    } else {
      updatedImages[item.imageIndex] = { ...image, flag };
      affectedIds.push({ id: image.id, oldFlag });
    }

    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      currentView,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    const flashColor =
      flag === "pick"
        ? "rgba(34, 197, 94, 0.15)"
        : flag === "reject"
          ? "rgba(239, 68, 68, 0.15)"
          : null;

    const undoEntry: UndoEntry = {
      imageId: image.id,
      field: "flag",
      oldValue: oldFlag,
      newValue: flag,
    };
    if (affectedIds.length > 1) {
      if (item.groupId && currentView === "triage" && item.isGroupCover) {
        undoEntry.batch = affectedIds.map((a) => ({
          imageId: a.id,
          oldValue: a.oldFlag,
          newValue: flag,
        }));
      } else if (item.groupId && currentView === "select" && flag === "pick") {
        undoEntry.batch = affectedIds.map((a) => ({
          imageId: a.id,
          oldValue: a.oldFlag,
          newValue: a.id === image.id ? "pick" : "reject",
        }));
      }
    }
    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      undoStack: [...undoStack.slice(-49), undoEntry],
      redoStack: [],
      lastFlagAction: flashColor
        ? { color: flashColor, timestamp: Date.now() }
        : get().lastFlagAction,
    });

    if (autoAdvance) {
      const clampedIndex = Math.min(currentIndex, Math.max(0, newDisplayItems.length - 1));
      setTimeout(() => {
        set({ currentIndex: clampedIndex, isZoomed: false });
      }, 150);
    } else {
      const clampedIndex = Math.min(currentIndex, Math.max(0, newDisplayItems.length - 1));
      if (clampedIndex !== currentIndex) {
        set({ currentIndex: clampedIndex });
      }
    }

    try {
      if (item.groupId && currentView === "triage" && item.isGroupCover) {
        const allIds = affectedIds.map((a) => a.id);
        await invoke("bulk_set_flag", { photoIds: allIds, flag });
      } else {
        await invoke("set_flag", { photoId: image.id, flag });
      }
    } catch (e) {
      console.error("Failed to set flag:", e);
      get().setToast(`Flag save failed: ${e}`, "error");
      const revertImages = [...get().images];
      for (const a of affectedIds) {
        const idx = revertImages.findIndex((img) => img.id === a.id);
        if (idx >= 0) {
          revertImages[idx] = { ...revertImages[idx], flag: a.oldFlag };
        }
      }
      set({
        images: revertImages,
        displayItems: computeDisplayItemsFiltered(
          revertImages,
          get().currentView,
          get().groups,
              get().activeInnerGroupId,
          selectRequiresPick(),
          routeMinStar(),
          currentAiOptions(get().sortByAi),
        ),
      });
    }
  },

  setDestination: async (dest: string) => {
    const { displayItems, currentIndex, undoStack, images, currentView, groups } =
      get();
    const item = displayItems[currentIndex];
    if (!item) return;

    const image = item.image;
    const oldDest = image.destination;
    if (oldDest === dest) return;

    const updatedImages = [...images];
    updatedImages[item.imageIndex] = { ...image, destination: dest };
    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      currentView,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      undoStack: [
        ...undoStack.slice(-49),
        {
          imageId: image.id,
          field: "destination",
          oldValue: oldDest,
          newValue: dest,
        },
      ],
      redoStack: [],
    });

    const clampedIndex = Math.min(currentIndex, newDisplayItems.length - 1);
    if (clampedIndex !== currentIndex && clampedIndex >= 0) {
      set({ currentIndex: clampedIndex });
    }

    try {
      await invoke("set_destination", {
        photoId: image.id,
        destination: dest,
      });
    } catch (e) {
      console.error("Failed to set destination:", e);
      get().setToast(`Destination save failed: ${e}`, "error");
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], destination: oldDest };
        set({
          images: revertImages,
          displayItems: computeDisplayItemsFiltered(
            revertImages,
            get().currentView,
            get().groups,
                  get().activeInnerGroupId,
            selectRequiresPick(),
            routeMinStar(),
            currentAiOptions(get().sortByAi),
          ),
        });
      }
    }
  },

  undo: async () => {
    const { undoStack, redoStack, images, currentView, groups } = get();
    const entry = undoStack[undoStack.length - 1];
    if (!entry) {
      get().setToast("Nothing to undo");
      return;
    }

    const updatedImages = [...images];
    const targets = entry.batch
      ? entry.batch.map((b) => ({ imageId: b.imageId, value: b.oldValue }))
      : [{ imageId: entry.imageId, value: entry.oldValue }];

    for (const t of targets) {
      const idx = updatedImages.findIndex((img) => img.id === t.imageId);
      if (idx < 0) continue;
      if (entry.field === "starRating") {
        updatedImages[idx] = { ...updatedImages[idx], starRating: t.value as number };
      } else if (entry.field === "flag") {
        updatedImages[idx] = { ...updatedImages[idx], flag: t.value as string };
      } else if (entry.field === "destination") {
        updatedImages[idx] = { ...updatedImages[idx], destination: t.value as string };
      }
    }

    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      currentView,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );
    const displayIdx = newDisplayItems.findIndex(
      (d) => d.image.id === entry.imageId,
    );

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      undoStack: undoStack.slice(0, -1),
      redoStack: [...redoStack, entry],
      currentIndex: displayIdx >= 0 ? displayIdx : 0,
    });

    try {
      for (const t of targets) {
        if (entry.field === "flag") {
          await invoke("set_flag", { photoId: t.imageId, flag: t.value });
        } else if (entry.field === "destination") {
          await invoke("set_destination", { photoId: t.imageId, destination: t.value });
        } else if (entry.field === "starRating") {
          await invoke("set_rating", { imageId: t.imageId, rating: t.value });
        }
      }
      get().setToast(describeUndoRedoEntry("Undo", entry));
    } catch (e) {
      console.error("Undo failed:", e);
      get().setToast(`Undo failed: ${e}`, "error");
    }
  },

  redo: async () => {
    const { redoStack, undoStack, images, currentView, groups } = get();
    const entry = redoStack[redoStack.length - 1];
    if (!entry) {
      get().setToast("Nothing to redo");
      return;
    }

    const updatedImages = [...images];
    const targets = entry.batch
      ? entry.batch.map((b) => ({ imageId: b.imageId, value: b.newValue }))
      : [{ imageId: entry.imageId, value: entry.newValue }];

    for (const t of targets) {
      const idx = updatedImages.findIndex((img) => img.id === t.imageId);
      if (idx < 0) continue;
      if (entry.field === "starRating") {
        updatedImages[idx] = { ...updatedImages[idx], starRating: t.value as number };
      } else if (entry.field === "flag") {
        updatedImages[idx] = { ...updatedImages[idx], flag: t.value as string };
      } else if (entry.field === "destination") {
        updatedImages[idx] = { ...updatedImages[idx], destination: t.value as string };
      }
    }

    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      currentView,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );
    const displayIdx = newDisplayItems.findIndex(
      (d) => d.image.id === entry.imageId,
    );

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      redoStack: redoStack.slice(0, -1),
      undoStack: [...undoStack, entry],
      currentIndex: displayIdx >= 0 ? displayIdx : 0,
    });

    try {
      for (const t of targets) {
        if (entry.field === "flag") {
          await invoke("set_flag", { photoId: t.imageId, flag: t.value });
        } else if (entry.field === "destination") {
          await invoke("set_destination", { photoId: t.imageId, destination: t.value });
        } else if (entry.field === "starRating") {
          await invoke("set_rating", { imageId: t.imageId, rating: t.value });
        }
      }
      get().setToast(describeUndoRedoEntry("Redo", entry));
    } catch (e) {
      console.error("Redo failed:", e);
      get().setToast(`Redo failed: ${e}`, "error");
    }
  },

  setView: async (view: CullView) => {
    const { currentShoot, displayItems, currentIndex, images, groups } = get();
    if (!currentShoot) return;

    const currentPhotoId = displayItems[currentIndex]?.image.id;
    if (currentPhotoId !== undefined) {
      invoke("set_view_cursor", {
        shootId: currentShoot.id,
        viewName: get().currentView,
        photoId: currentPhotoId,
      }).catch(() => {});
    }

    const newDisplayItems = computeDisplayItemsFiltered(
      images,
      view,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    let newIndex = 0;
    try {
      const cursor = await invoke<number | null>("get_view_cursor", {
        shootId: currentShoot.id,
        viewName: view,
      });
      if (cursor !== null) {
        const idx = newDisplayItems.findIndex((d) => d.image.id === cursor);
        if (idx >= 0) newIndex = idx;
      }
    } catch {
      // no saved cursor
    }

    set({
      currentView: view,
      displayItems: newDisplayItems,
      currentIndex: newIndex,
      isZoomed: false,
      showMetadata: view === "route" ? true : get().showMetadata,
    });
  },

  setViewMode: (mode: ViewMode) => set({ viewMode: mode }),

  advanceToNextUnreviewed: () => {
    const { displayItems, currentIndex } = get();
    for (let i = currentIndex + 1; i < displayItems.length; i++) {
      if (displayItems[i].image.flag === "unreviewed") {
        set({ currentIndex: i, isZoomed: false });
        return;
      }
    }
    for (let i = 0; i < currentIndex; i++) {
      if (displayItems[i].image.flag === "unreviewed") {
        set({ currentIndex: i, isZoomed: false });
        return;
      }
    }
  },

  clearFlagFlash: () => set({ lastFlagAction: null }),

  setToast: (message: string, kind: "info" | "error" = "info") =>
    set({ toast: { message, kind, timestamp: Date.now() } }),
  clearToast: () => set({ toast: null }),

  toggleMetadata: () => set((s) => ({ showMetadata: !s.showMetadata })),
  toggleShortcutHints: () =>
    set((s) => ({ showShortcutHints: !s.showShortcutHints })),
  toggleAiPanel: () => set((s) => ({ aiPanelForced: !s.aiPanelForced })),
  toggleHeatmap: () => set((s) => ({ heatmapOn: !s.heatmapOn })),

  getHeatmapData: (photoId: number) => {
    const cached = get().heatmapCache.get(photoId);
    if (cached) return cached;
    invoke<number[]>("get_heatmap", { photoId })
      .then((grid) => {
        const next = new Map(get().heatmapCache);
        // Cap cache at 20 entries (FIFO eviction).
        while (next.size >= 20) {
          const firstKey = next.keys().next().value;
          if (firstKey === undefined) break;
          next.delete(firstKey);
        }
        next.set(photoId, grid);
        set({ heatmapCache: next });
      })
      .catch(() => {
        // Silently fail — the overlay just stays empty.
      });
    return null;
  },
  toggleAutoAdvance: () => set((s) => ({ autoAdvance: !s.autoAdvance })),
  toggleZoom: () => set((s) => ({ isZoomed: !s.isZoomed })),

  setActiveInnerGroup: (groupId: number | null) => {
    const { activeInnerGroupId, images, currentView, groups, displayItems, currentIndex } = get();
    // Always-set semantics: passing the already-active id is a no-op,
    // not a toggle. Callers that want to close the inner strip pass
    // `null` explicitly. This keeps "single-click to expand" from
    // accidentally collapsing when the user taps the same cover twice.
    if (groupId === activeInnerGroupId) return;
    const nextActive = groupId;

    const currentPhotoId = displayItems[currentIndex]?.image.id;
    const newDisplayItems = computeDisplayItemsFiltered(
      images,
      currentView,
      groups,
      nextActive,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    let newIndex = 0;
    if (currentPhotoId !== undefined) {
      const idx = newDisplayItems.findIndex((d) => d.image.id === currentPhotoId);
      if (idx >= 0) newIndex = idx;
    }
    newIndex = Math.min(newIndex, Math.max(0, newDisplayItems.length - 1));

    set({
      activeInnerGroupId: nextActive,
      displayItems: newDisplayItems,
      currentIndex: newIndex < 0 ? 0 : newIndex,
    });
  },

  setFlagNoAutoReject: async (flag: string) => {
    const { displayItems, currentIndex, autoAdvance, undoStack, images, currentView, groups } = get();
    const item = displayItems[currentIndex];
    if (!item) return;

    const image = item.image;
    const oldFlag = image.flag;
    if (oldFlag === flag) return;

    const updatedImages = [...images];
    updatedImages[item.imageIndex] = { ...image, flag };
    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      currentView,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    const flashColor = flag === "pick" ? "rgba(34, 197, 94, 0.15)" : flag === "reject" ? "rgba(239, 68, 68, 0.15)" : null;

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      undoStack: [...undoStack.slice(-49), { imageId: image.id, field: "flag", oldValue: oldFlag, newValue: flag }],
      redoStack: [],
      lastFlagAction: flashColor ? { color: flashColor, timestamp: Date.now() } : get().lastFlagAction,
    });

    if (autoAdvance) {
      const clampedIndex = Math.min(currentIndex, Math.max(0, newDisplayItems.length - 1));
      setTimeout(() => {
        set({ currentIndex: clampedIndex, isZoomed: false });
      }, 150);
    }

    try {
      await invoke("set_flag", { photoId: image.id, flag });
    } catch (e) {
      console.error("Failed to set flag:", e);
      get().setToast(`Flag save failed: ${e}`, "error");
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], flag: oldFlag };
        set({
          images: revertImages,
          displayItems: computeDisplayItemsFiltered(
            revertImages,
            get().currentView,
            get().groups,
                  get().activeInnerGroupId,
            selectRequiresPick(),
            routeMinStar(),
            currentAiOptions(get().sortByAi),
          ),
        });
      }
    }
  },

  setGroupCover: async (groupId: number, photoId: number) => {
    const { groups } = get();
    const updatedGroups = groups.map((g) => {
      if (g.id !== groupId) return g;
      return {
        ...g,
        members: g.members.map((m) => ({ ...m, isCover: m.photoId === photoId })),
      };
    });

    set({ groups: updatedGroups });

    try {
      await invoke("set_group_cover", { groupId, photoId });
    } catch (e) {
      console.error("Failed to set group cover:", e);
      get().setToast(`Set cover failed: ${e}`, "error");
      set({ groups });
    }
  },

  acceptAiPick: async () => {
    const { groups, images, displayItems, currentIndex, setGroupCover } = get();
    const item = displayItems[currentIndex];
    if (!item?.groupId) return;
    const group = groups.find((g) => g.id === item.groupId);
    if (!group) return;
    const pickId = aiPickForGroup(group, images);
    if (pickId === null) return;
    await setGroupCover(group.id, pickId);
  },

  getGroupForCurrentItem: () => {
    const { displayItems, currentIndex, groups } = get();
    const item = displayItems[currentIndex];
    if (!item?.groupId) return null;
    return groups.find((g) => g.id === item.groupId) ?? null;
  },

  enterComparison: () => {
    const { displayItems, currentIndex, groups, currentView, images } = get();
    if (currentView !== "select") return;
    const item = displayItems[currentIndex];
    if (!item?.groupId) return;

    const group = groups.find((g) => g.id === item.groupId);
    if (!group) return;

    const memberIds = group.members
      .filter((m) => {
        const img = images.find((i) => i.id === m.photoId);
        return img && img.flag !== "reject";
      })
      .map((m) => m.photoId);

    if (memberIds.length < 2) return;

    const pinnedId = item.image.id;
    const cyclingId = memberIds.find((id) => id !== pinnedId) ?? memberIds[0];

    set({
      viewMode: "comparison",
      comparisonPinnedId: pinnedId,
      comparisonCyclingId: cyclingId,
      comparisonGroupMembers: memberIds,
    });
  },

  exitComparison: () => {
    const { comparisonPinnedId, displayItems } = get();
    const idx = displayItems.findIndex(
      (d) => d.image.id === comparisonPinnedId,
    );
    set({
      viewMode: "sequential",
      currentIndex: idx >= 0 ? idx : 0,
      comparisonPinnedId: null,
      comparisonCyclingId: null,
      comparisonGroupMembers: [],
    });
  },

  cycleComparison: (direction: 1 | -1) => {
    const { comparisonCyclingId, comparisonPinnedId, comparisonGroupMembers, images } = get();
    const available = comparisonGroupMembers.filter((id) => {
      if (id === comparisonPinnedId) return false;
      const img = images.find((i) => i.id === id);
      return img && img.flag !== "reject";
    });
    if (available.length === 0) return;

    const curIdx = available.indexOf(comparisonCyclingId!);
    let nextIdx = curIdx + direction;
    if (nextIdx < 0) nextIdx = available.length - 1;
    if (nextIdx >= available.length) nextIdx = 0;

    set({ comparisonCyclingId: available[nextIdx] });
  },

  comparisonQuickPick: async (side: "left" | "right") => {
    const { comparisonPinnedId, comparisonCyclingId, images, currentView, groups, undoStack } = get();
    if (!comparisonPinnedId || !comparisonCyclingId) return;

    const pickId = side === "left" ? comparisonPinnedId : comparisonCyclingId;
    const rejectId = side === "left" ? comparisonCyclingId : comparisonPinnedId;

    const updatedImages = [...images];
    const pickIdx = updatedImages.findIndex((i) => i.id === pickId);
    const rejectIdx = updatedImages.findIndex((i) => i.id === rejectId);
    if (pickIdx < 0 || rejectIdx < 0) return;

    const oldPickFlag = updatedImages[pickIdx].flag;
    const oldRejectFlag = updatedImages[rejectIdx].flag;
    updatedImages[pickIdx] = { ...updatedImages[pickIdx], flag: "pick" };
    updatedImages[rejectIdx] = { ...updatedImages[rejectIdx], flag: "reject" };

    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      currentView,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      undoStack: [
        ...undoStack.slice(-49),
        {
          imageId: pickId,
          field: "flag",
          oldValue: oldPickFlag,
          newValue: "pick",
          batch: [
            { imageId: pickId, oldValue: oldPickFlag, newValue: "pick" },
            { imageId: rejectId, oldValue: oldRejectFlag, newValue: "reject" },
          ],
        },
      ],
      redoStack: [],
      lastFlagAction: { color: "rgba(34, 197, 94, 0.15)", timestamp: Date.now() },
    });

    try {
      await invoke("set_flag", { photoId: pickId, flag: "pick" });
      await invoke("set_flag", { photoId: rejectId, flag: "reject" });
    } catch (e) {
      console.error("Failed quick pick:", e);
      get().setToast(`Quick pick failed: ${e}`, "error");
    }

    const available = get().comparisonGroupMembers.filter((id) => {
      if (id === comparisonPinnedId) return false;
      const img = get().images.find((i) => i.id === id);
      return img && img.flag !== "reject";
    });
    if (available.length < 1) {
      get().exitComparison();
    } else {
      const newCycling = available.find((id) => id !== rejectId) ?? available[0];
      set({ comparisonCyclingId: newCycling });
    }
  },

  createGroupFromPhotos: async (photoIds: number[]) => {
    const { currentShoot, images, currentView } = get();
    if (!currentShoot || photoIds.length < 2) return;
    try {
      await invoke("create_group_from_photos", {
        shootId: currentShoot.id,
        photoIds,
        groupType: "near_duplicate",
      });
      const groups = await invoke<Group[]>("get_groups_for_shoot", {
        shootId: currentShoot.id,
      });
      set({
        groups,
        displayItems: computeDisplayItemsFiltered(
          images,
          currentView,
          groups,
              get().activeInnerGroupId,
          selectRequiresPick(),
          routeMinStar(),
          currentAiOptions(get().sortByAi),
        ),
      });
    } catch (e) {
      console.error("Create group failed:", e);
      get().setToast(`Group failed: ${e}`, "error");
    }
  },

  refreshDisplay: () => {
    const { images, currentView, groups, displayItems, currentIndex } = get();
    const currentPhotoId = displayItems[currentIndex]?.image.id;
    const next = computeDisplayItemsFiltered(
      images,
      currentView,
      groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );
    let nextIndex = currentIndex;
    if (currentPhotoId !== undefined) {
      const idx = next.findIndex((d) => d.image.id === currentPhotoId);
      if (idx >= 0) nextIndex = idx;
    }
    nextIndex = Math.min(nextIndex, Math.max(0, next.length - 1));
    set({ displayItems: next, currentIndex: nextIndex < 0 ? 0 : nextIndex });
  },

  // Called from the ai-progress listener: pulls the latest AI fields
  // for one photo from the backend and patches the local images array
  // so the UI reflects analysis results as they land. Without this,
  // face_count / sharpness_score stay at whatever they were when
  // loadShoot snapshotted the DB — typically null — and the panel,
  // badges, and sort never see real data.
  patchImageAiData: async (photoId: number) => {
    const idx = get().images.findIndex((i) => i.id === photoId);
    if (idx < 0) return; // photo not in the current shoot's view
    try {
      const fresh = await invoke<ImageEntry>("get_image_metadata", { imageId: photoId });
      const updatedImages = [...get().images];
      updatedImages[idx] = {
        ...updatedImages[idx],
        faceCount: fresh.faceCount,
        eyesOpenCount: fresh.eyesOpenCount,
        sharpnessScore: fresh.sharpnessScore,
        aiAnalyzedAt: fresh.aiAnalyzedAt,
      };
      set({ images: updatedImages });
      get().refreshDisplay();
    } catch (e) {
      console.error("patchImageAiData failed for", photoId, e);
    }
  },

  appendImportedPhoto: async (photoId: number) => {
    const existing = get().images.findIndex((i) => i.id === photoId);
    if (existing >= 0) return; // already in the list (raced with loadShoot)
    try {
      const fresh = await invoke<ImageEntry>("get_image_metadata", { imageId: photoId });
      // Preserve capture-time order to match what `loadShoot` produced.
      // Most imports arrive in capture order, so this is usually a
      // no-op O(1) append; a mis-ordered batch still settles cheaply.
      const merged = [...get().images, fresh].sort((a, b) => {
        const at = a.captureTime ?? "";
        const bt = b.captureTime ?? "";
        if (at && bt && at !== bt) return at < bt ? -1 : 1;
        if (at && !bt) return -1;
        if (!at && bt) return 1;
        return a.id - b.id;
      });
      set({ images: merged });
      get().refreshDisplay();
    } catch (e) {
      console.error("appendImportedPhoto failed for", photoId, e);
    }
  },

  refetchGroups: async () => {
    const shoot = get().currentShoot;
    if (!shoot) return;
    try {
      const groups = await invoke<Group[]>("get_groups_for_shoot", { shootId: shoot.id });
      set({ groups });
      get().refreshDisplay();
    } catch (e) {
      console.error("refetchGroups failed:", e);
    }
  },

  cycleSortByAi: () => {
    const cur = get().sortByAi;
    const next: "none" | "sharpness" | "faces" =
      cur === "none" ? "sharpness" : cur === "sharpness" ? "faces" : "none";
    set({ sortByAi: next });
    get().refreshDisplay();
  },

  ungroupPhotos: async (photoIds: number[]) => {
    const { currentShoot, images, currentView } = get();
    if (!currentShoot || photoIds.length === 0) return;
    try {
      await invoke("ungroup_photos", { photoIds });
      const groups = await invoke<Group[]>("get_groups_for_shoot", {
        shootId: currentShoot.id,
      });
      set({
        groups,
        displayItems: computeDisplayItemsFiltered(
          images,
          currentView,
          groups,
              get().activeInnerGroupId,
          selectRequiresPick(),
          routeMinStar(),
          currentAiOptions(get().sortByAi),
        ),
      });
    } catch (e) {
      console.error("Ungroup failed:", e);
      get().setToast(`Ungroup failed: ${e}`, "error");
    }
  },
}));

// Settings that feed `computeDisplayItems` are read lazily at call time
// via the `selectRequiresPick()` / `routeMinStar()` / `hideSoftThreshold()`
// helpers above — but updating a setting in SettingsDialog only mutates
// `settingsStore`; it doesn't trigger a re-compute here. Subscribe so
// the user sees the filter / sort take effect as soon as they adjust
// the slider.
let lastHideSoft = useSettingsStore.getState().settings.hideSoftThreshold;
let lastSelectRequiresPick = useSettingsStore.getState().settings.selectRequiresPick;
let lastRouteMinStar = useSettingsStore.getState().settings.routeMinStar;
useSettingsStore.subscribe((state) => {
  const {
    hideSoftThreshold: hs,
    selectRequiresPick: sr,
    routeMinStar: rm,
  } = state.settings;
  if (
    hs !== lastHideSoft ||
    sr !== lastSelectRequiresPick ||
    rm !== lastRouteMinStar
  ) {
    lastHideSoft = hs;
    lastSelectRequiresPick = sr;
    lastRouteMinStar = rm;
    useProjectStore.getState().refreshDisplay();
  }
});
