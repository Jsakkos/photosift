import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  ImageEntry,
  ShootSummary,
  CullView,
  ViewMode,
  DisplayItem,
  Group,
  SyncReport,
} from "../types";
import { useSettingsStore } from "./settingsStore";
import { useAiStore } from "./aiStore";
import {
  createBracket,
  applyDecision,
  currentPair,
  type BracketState,
  type Decision,
} from "../lib/bracket";

function selectRequiresPick(): boolean {
  return useSettingsStore.getState().settings.selectRequiresPick ?? false;
}

/// Pick the pass the user should land in when they reopen a shoot.
/// Cascades forward from the seed view (usually `shoot.lastView`) and
/// skips any pass whose work is done. Never demotes: if they left off in
/// Route we don't drag them back to Select just because a newer photo
/// landed unreviewed. The completion predicates mirror the filters used
/// by `passesSelectGate` and the Route view so this stays honest as those
/// evolve.
function computeActivePass(images: ImageEntry[], start: CullView): CullView {
  const hasTriageWork = images.some((i) => i.flag === "unreviewed");
  const hasSelectWork = images.some(
    (i) => i.flag === "pick" && i.starRating === 0,
  );
  const hasRouteWork = images.some(
    (i) =>
      i.flag === "pick" &&
      i.starRating >= 3 &&
      i.destination === "unrouted",
  );
  if (start === "triage" && hasTriageWork) return "triage";
  if ((start === "triage" || start === "select") && hasSelectWork)
    return "select";
  if (hasRouteWork) return "route";
  return start;
}

/// Map a view transition to the layout-sync trigger name the Rust side
/// knows about, or `null` when the transition doesn't finalize a pass.
/// Leaving triage for anything finalizes rejects; leaving select *for
/// route specifically* finalizes selects (matches the user's mental
/// model: "I'm moving on to route now"); leaving route finalizes routed
/// picks into edit/export. Returning to a pass view never finalizes —
/// that would move files back prematurely.
function resolveSyncTrigger(from: CullView, to: CullView): string | null {
  if (from === "triage" && to !== "triage") return "triage_complete";
  if (from === "select" && to === "route") return "select_complete";
  if (from === "route" && to !== "route") return "route_complete";
  return null;
}

function routeMinStar(): number {
  return useSettingsStore.getState().settings.routeMinStar ?? 0;
}

/// Session-scoped "pass floor" for Select. Read lazily so every
/// computeDisplayItemsFiltered call picks up the current tier without
/// threading the value through 18+ internal callers. Defaults to 0 on
/// first read (Pass 1 — show everything pick-flagged).
function currentSelectMinStar(): number {
  return useProjectStore.getState().selectMinStar ?? 0;
}

/// Bundle the sort/pick options that `computeDisplayItems` needs.
/// Centralized so call sites don't have to remember to keep
/// `useEyesInPick` / `useSmileInPick` gated on the provider kinds — if the
/// backend swaps to a real classifier, this helper picks it up automatically.
function currentAiOptions(
  sortByAi: "none" | "sharpness" | "faces",
): AiDisplayOptions {
  const { eyeProvider, mouthProvider } = useAiStore.getState();
  return {
    sortByAi,
    useEyesInPick: eyeProvider === "onnx",
    useSmileInPick: mouthProvider === "onnx",
  };
}

/// Reads the store flag that, when true, disables the triage/select
/// review filter so picked/rejected photos remain visible alongside
/// unreviewed ones. Lets the user recover from an accidental P/X.
function showReviewedFlag(): boolean {
  return useProjectStore.getState().showReviewed;
}

/// Read a persisted boolean from localStorage, defaulting on first run
/// or when JSON.parse throws (corrupt value, SSR-like env without
/// localStorage). Used for session-scoped UI flags like rail visibility.
function readBoolLS(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    if (v === null) return fallback;
    return JSON.parse(v) === true;
  } catch {
    return fallback;
  }
}

function writeBoolLS(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Non-fatal — rail visibility falls back to per-session state.
  }
}

export interface AiDisplayOptions {
  sortByAi: "none" | "sharpness" | "faces";
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
    // When present, overrides the parent `field` for this item — lets
    // a single undo entry mix multiple field types (e.g. bracket
    // decisions bundle several star promotions). Falls back to parent
    // `field` otherwise.
    field?: "starRating" | "flag" | "destination";
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
/// Finds the outer-list index of a group's cover tile, or -1 if the
/// group has no remaining unreviewed members (so no cover is emitted).
/// Used by setFlag / setFlagNoAutoReject to remember where the user's
/// drilled-in group SAT in the outer list *before* they finished it —
/// after auto-exit the same index now holds whatever came next, so the
/// post-advance cursor lands adjacent instead of snapping to index 0.
function outerIndexOfGroupCover(
  images: ImageEntry[],
  currentView: CullView,
  groups: Group[],
  groupId: number,
  aiOptions: AiDisplayOptions,
): number {
  const outer = computeDisplayItemsFiltered(
    images,
    currentView,
    groups,
    null,
    selectRequiresPick(),
    routeMinStar(),
    aiOptions,
  );
  return outer.findIndex((d) => d.isGroupCover && d.groupId === groupId);
}

/// Compute the display items AND figure out whether the drill-down
/// should auto-exit. When every member of the currently-drilled group
/// has been picked/rejected (as happens after the user finishes
/// triaging a burst), the raw drill-down computation returns [] — but
/// `CullPage` treats `displayItems.length === 0` as "triage complete"
/// and renders EmptyViewState, hiding the 40 unreviewed photos in
/// other groups. Detect that and fall back to the outer list, also
/// returning the new activeInnerGroupId so callers can clear it.
function computeWithAutoExit(
  images: ImageEntry[],
  currentView: CullView,
  groups: Group[],
  activeInnerGroupId: number | null,
  aiOptions: AiDisplayOptions = DEFAULT_AI_OPTIONS,
): { items: DisplayItem[]; activeInnerGroupId: number | null } {
  const items = computeDisplayItemsFiltered(
    images,
    currentView,
    groups,
    activeInnerGroupId,
    selectRequiresPick(),
    routeMinStar(),
    aiOptions,
  );
  if (activeInnerGroupId != null && items.length === 0) {
    const outer = computeDisplayItemsFiltered(
      images,
      currentView,
      groups,
      null,
      selectRequiresPick(),
      routeMinStar(),
      aiOptions,
    );
    return { items: outer, activeInnerGroupId: null };
  }
  return { items, activeInnerGroupId };
}

function computeDisplayItemsFiltered(
  images: ImageEntry[],
  currentView: CullView,
  groups: Group[],
  activeInnerGroupId: number | null,
  selectRequiresPickFilter: boolean,
  routeMinStarGate: number,
  aiOptions: AiDisplayOptions = DEFAULT_AI_OPTIONS,
): DisplayItem[] {
  // Reads the toggle lazily so every call site automatically picks up
  // the user's current Show-all preference without having to thread the
  // flag through the 17+ store actions that recompute displayItems.
  const showReviewed = showReviewedFlag();
  const selectMinStar = currentSelectMinStar();
  if (activeInnerGroupId == null) {
    return computeDisplayItems(
      images,
      currentView,
      groups,
      new Set<number>(),
      selectRequiresPickFilter,
      routeMinStarGate,
      aiOptions,
      showReviewed,
      selectMinStar,
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

    if (!showReviewed) {
      if (currentView === "triage") {
        // Filter picked/rejected so the reviewed photo actually disappears.
        // The prior AI-pick pin kept the recommended frame visible after
        // P/X, but users read that as "my pick didn't register" — flip
        // the default and rely on the Show-all toggle for second thoughts.
        if (img.flag !== "unreviewed") continue;
      } else if (currentView === "select") {
        const passesFlag = selectRequiresPickFilter
          ? img.flag === "pick"
          : img.flag !== "reject";
        if (!passesFlag) continue;
        if (img.starRating < selectMinStar) continue;
      } else {
        // route view
        if (img.flag !== "pick" || img.destination !== "unrouted") continue;
        if (routeMinStarGate > 0 && img.starRating < routeMinStarGate) continue;
      }
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
/// Ranks by persisted `quality_score` (Rust worker's composite of
/// sharpness + subject presence + eye-open ratio + smile). This is the
/// same number drives the #rank badge in InnerStrip, so ★ AI and #1
/// always agree. Falls back to `sharpnessScore` when a photo was
/// analyzed before quality_score existed (pre-2026-04-20 shoots).
/// Ties broken by lower id.
///
/// The `useEyes` / `useSmile` params are retained for backwards
/// compatibility but no longer affect ranking — quality_score already
/// incorporates those signals at write time if the provider reported
/// them. Kept so existing callers don't need to be updated in one pass.
export function aiPickForGroup(
  group: Group,
  images: ImageEntry[],
  _useEyes: boolean = false,
  _useSmile: boolean = false,
): number | null {
  void _useEyes;
  void _useSmile;
  const analyzed = group.members
    .map((m) => images.find((i) => i.id === m.photoId))
    .filter((img): img is ImageEntry => !!img && img.aiAnalyzedAt != null);

  if (analyzed.length < 2) return null;

  const scoreOf = (img: ImageEntry): number =>
    typeof img.qualityScore === "number"
      ? img.qualityScore
      : (img.sharpnessScore ?? 0);

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
  showReviewed: boolean = false,
  selectMinStar: number = 0,
): DisplayItem[] {
  const items: DisplayItem[] = [];
  const photoGroupMap = buildPhotoGroupMap(groups);

  if (currentView === "triage") {
    const seenGroups = new Set<number>();
    // In triage, a photo passes unless it's already picked/rejected —
    // unless the user flipped the Show-all toggle in ViewSelector,
    // which re-includes reviewed photos so they can un-pick/un-reject.
    const passesTriage = (img: ImageEntry): boolean =>
      showReviewed || img.flag === "unreviewed";

    for (let i = 0; i < images.length; i++) {
      const img = images[i];
      if (!passesTriage(img)) continue;

      const group = photoGroupMap.get(img.id);
      if (group) {
        if (seenGroups.has(group.id)) continue;
        seenGroups.add(group.id);
        if (expandedGroupIds.has(group.id)) {
          // Drill-down inline: emit each member whose flag still passes.
          // No AI-pick pinning — picked/rejected photos leave the list
          // so the user sees their action take effect.
          for (const member of group.members) {
            const mi = images.findIndex((im) => im.id === member.photoId);
            if (mi < 0) continue;
            const memImg = images[mi];
            if (!passesTriage(memImg)) continue;
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
        const visibleCount = group.members.filter((m) => {
          const mi = images.find((im) => im.id === m.photoId);
          return mi && passesTriage(mi);
        }).length;
        if (visibleCount === 0) continue;
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
    const passesSelectGate = (img: ImageEntry): boolean => {
      // showReviewed bypasses the flag filter (so picked/rejected become
      // visible again) but NOT the pass floor — the floor is a workflow
      // tier chosen by the user, not a review filter, so leaving it
      // enforced here keeps "Show all" from silently breaking Pass 2+.
      const passesFlag = showReviewed
        ? true
        : selectRequiresPickFilter
          ? img.flag === "pick"
          : img.flag !== "reject";
      return passesFlag && img.starRating >= selectMinStar;
    };

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
      const starGate =
        routeMinStarGate === 0 || img.starRating >= routeMinStarGate;
      if (showReviewed) {
        if (img.flag === "pick" && starGate) {
          items.push({ imageIndex: i, image: img });
        }
      } else if (
        img.flag === "pick" &&
        img.destination === "unrouted" &&
        starGate
      ) {
        items.push({ imageIndex: i, image: img });
      }
    }
  }

  let result = items;

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
  /// Triage/Select rail visibility. Persisted via localStorage so the
  /// user's last T/F toggle state survives reloads. Not per-shoot.
  showAllStrip: boolean;
  showFaces: boolean;
  toggleAllStrip: () => void;
  toggleFaces: () => void;
  /// Triage-only bulk keep. Picks every member of the currently-focused
  /// photo's burst group, persists via `bulk_set_flag`, and records a
  /// single batch undo entry so one Z undoes the whole group.
  keepAllInCurrentGroup: () => Promise<void>;

  loadShoot: (shootId: number) => Promise<void>;
  /// Narrative-Select-style drilldown. Pass a groupId to open the inner
  /// strip for that group; pass the same id to toggle closed; pass null
  /// to clear. Only one group can be active at a time.
  setActiveInnerGroup: (groupId: number | null) => void;
  /// If the current Triage/Select cursor sits on a group cover and we're
  /// not already drilled in, drill in. Called after every navigation /
  /// view-switch / load so the user reviews group members individually
  /// instead of landing on a cover and accidentally mass-triaging.
  autoDrillIfOnCover: () => void;
  setCurrentIndex: (index: number) => void;
  navigateNext: () => void;
  navigatePrev: () => void;
  setRating: (rating: number) => Promise<void>;
  setFlag: (flag: string) => Promise<void>;
  setDestination: (dest: string) => Promise<void>;
  /// Apply `dest` to every photo in `photoIds` in one batch. Persists each
  /// via `set_destination`, updates local state once, and records a single
  /// undo entry so Ctrl+Z reverts the whole bulk action. Used by Route's
  /// "Route selected" / "Route all" buttons.
  bulkSetDestination: (photoIds: number[], dest: string) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
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
  /// 2-up tournament bracket state for Select. Null when not in 2-up
  /// mode. Entered automatically when the cursor lands on a group cover
  /// with ≥2 reviewable members; exited when the bracket completes or
  /// the user toggles Tab. See `src/lib/bracket.ts` for the engine.
  selectBracket: BracketState | null;
  /// Group id the user explicitly toggled 2-up off for via Tab. Tracked
  /// per-group so the next group still auto-enters 2-up even if the
  /// previous one was manually suppressed.
  selectBracketSuppressedForGroup: number | null;
  /// Photo ids the user has focused at least once at the current
  /// `selectMinStar` floor. Reset whenever the floor changes. When every
  /// visible displayItem is in the set, the floor auto-bumps and the
  /// cursor resets to 0.
  selectVisitedAtFloor: Set<number>;
  enterBracket: () => void;
  exitBracket: () => void;
  bracketDecision: (decision: Decision) => Promise<void>;
  /// Space in single-photo mode: +1 star (clamped at 5) + advance.
  pickCurrent: () => Promise<void>;
  /// → in single-photo mode: advance without mutating rating.
  skipCurrent: () => void;
  /// Record a Select-mode visit. Fires pass-complete detection.
  markVisitedAtFloor: (photoId: number) => void;
  /// Bump floor + reset cursor when every visible photo has been
  /// visited at the current tier. Internal helper; call sites use
  /// `markVisitedAtFloor` which invokes this.
  maybeBumpFloor: () => void;
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
  /// When true, the review filters in all three views are bypassed so
  /// picked/rejected/routed photos remain visible. Lets the user walk
  /// back an accidental P/X without hunting the Undo shortcut.
  showReviewed: boolean;
  toggleShowReviewed: () => void;
  /// Pass floor for Select — the minimum star rating a photo needs to
  /// be visible. Session-scoped; not persisted per shoot. Starts at 0
  /// (Pass 1, show everything pick-flagged). `]` / `[` and the pass
  /// chips adjust it; after-rating auto-advance bumps by 1 when every
  /// visible photo has already cleared the next tier.
  selectMinStar: number;
  setSelectMinStar: (n: number) => void;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  currentShoot: null,
  images: [],
  currentIndex: 0,
  isLoading: false,
  loadError: null,
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
  selectBracket: null,
  selectBracketSuppressedForGroup: null,
  selectVisitedAtFloor: new Set<number>(),
  sortByAi: "none" as const,
  heatmapOn: false,
  heatmapCache: new Map<number, number[]>(),
  showReviewed: false,
  selectMinStar: 0,
  // Immersive Select mode defaults to a hidden filmstrip; users opt in
  // with T. localStorage still wins if they previously toggled it on.
  showAllStrip: readBoolLS("photosift.rail.allStrip", false),
  showFaces: readBoolLS("photosift.rail.faces", true),

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

      // Resume in whichever view the user last opened for this shoot,
      // then cascade past any pass that has no work left. Skipping
      // completed passes on reopen means users land where they can
      // actually act (e.g. finished triage ➜ Select) instead of on an
      // empty list.
      const seedView: CullView = shoot.lastView ?? "triage";
      const resumeView: CullView = computeActivePass(images, seedView);

      const cursor = await invoke<number | null>("get_view_cursor", {
        shootId,
        viewName: resumeView,
      }).catch(() => null);

      // Reset the Select pass floor to 0 before computing the initial
      // display — otherwise a leftover `selectMinStar` from the previous
      // shoot in the same session would filter this new shoot's photos
      // at the wrong tier, making the loupe load on an empty list.
      useProjectStore.setState({ selectMinStar: 0 });

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
        // New shoot → restart at Pass 1. The floor is session-scoped
        // but carrying it across shoots would land the user on an
        // empty tier whenever they open a fresh shoot, which reads as
        // "Select is broken."
        selectMinStar: 0,
      });
      get().autoDrillIfOnCover();

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
      get().autoDrillIfOnCover();
      const landed = get().displayItems[get().currentIndex]?.image.id;
      if (landed != null) get().markVisitedAtFloor(landed);
    }
  },

  navigateNext: () => {
    const { currentIndex, displayItems, activeInnerGroupId } = get();
    if (currentIndex < displayItems.length - 1) {
      set({ currentIndex: currentIndex + 1, isZoomed: false });
      get().autoDrillIfOnCover();
      const landed = get().displayItems[get().currentIndex]?.image.id;
      if (landed != null) get().markVisitedAtFloor(landed);
      return;
    }
    // End of the drilled-in group: step out into the flat view and land
    // on whatever follows this group's cover. `autoDrillIfOnCover` then
    // pulls us into the next burst if there is one, so arrow-through
    // traversal is seamless across group boundaries.
    if (activeInnerGroupId !== null) {
      const { images, currentView, groups, sortByAi } = get();
      const coverIdx = outerIndexOfGroupCover(
        images,
        currentView,
        groups,
        activeInnerGroupId,
        currentAiOptions(sortByAi),
      );
      get().setActiveInnerGroup(null);
      const fresh = get();
      const targetIdx = coverIdx + 1;
      if (targetIdx >= 0 && targetIdx < fresh.displayItems.length) {
        set({ currentIndex: targetIdx, isZoomed: false });
        get().autoDrillIfOnCover();
      }
    }
  },

  navigatePrev: () => {
    const { currentIndex, activeInnerGroupId } = get();
    if (currentIndex > 0) {
      set({ currentIndex: currentIndex - 1, isZoomed: false });
      get().autoDrillIfOnCover();
      const landed = get().displayItems[get().currentIndex]?.image.id;
      if (landed != null) get().markVisitedAtFloor(landed);
      return;
    }
    // Start of the drilled-in group: step out into the flat view and
    // land on whatever precedes this group's cover, mirroring the
    // forward traversal so arrow keys never dead-end at a drill edge.
    if (activeInnerGroupId !== null) {
      const { images, currentView, groups, sortByAi } = get();
      const coverIdx = outerIndexOfGroupCover(
        images,
        currentView,
        groups,
        activeInnerGroupId,
        currentAiOptions(sortByAi),
      );
      get().setActiveInnerGroup(null);
      const targetIdx = coverIdx - 1;
      if (targetIdx >= 0) {
        set({ currentIndex: targetIdx, isZoomed: false });
        get().autoDrillIfOnCover();
      }
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

    // Mark visited at floor *before* advancing — the photo we just
    // rated counts as reviewed for this pass.
    if (get().currentView === "select") {
      const visited = new Set(get().selectVisitedAtFloor);
      visited.add(image.id);
      set({ selectVisitedAtFloor: visited });
    }

    if (autoAdvance && currentIndex < newDisplayItems.length - 1) {
      set({ currentIndex: currentIndex + 1, isZoomed: false });
    }

    // Pass-complete check: if every visible photo at the current floor
    // has been visited, bump the floor and reset the cursor.
    if (get().currentView === "select") {
      get().maybeBumpFloor();
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

    // If drilled in, remember where the active group's cover sits in
    // the outer list so auto-advance after a drill-empty can snap to
    // the adjacent item (post-removal, the same index now points at
    // the next group or standalone photo).
    const preActiveId = get().activeInnerGroupId;
    const preDrilledOuterIdx =
      preActiveId != null
        ? outerIndexOfGroupCover(
            images,
            currentView,
            groups,
            preActiveId,
            currentAiOptions(get().sortByAi),
          )
        : -1;

    const updatedImages = [...images];
    const affectedIds: { id: number; oldFlag: string }[] = [];

    if (item.groupId && currentView === "select" && flag === "pick") {
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

    const { items: newDisplayItems, activeInnerGroupId: newActive } =
      computeWithAutoExit(
        updatedImages,
        currentView,
        groups,
        get().activeInnerGroupId,
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
      if (item.groupId && currentView === "select" && flag === "pick") {
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
      activeInnerGroupId: newActive,
      undoStack: [...undoStack.slice(-49), undoEntry],
      redoStack: [],
      lastFlagAction: flashColor
        ? { color: flashColor, timestamp: Date.now() }
        : get().lastFlagAction,
    });

    // If this triage action just cleared the last unreviewed photo,
    // cross the view boundary to Select so the user doesn't get stuck
    // on EmptyViewState. Only fires on a real triage transition
    // (unreviewed → pick/reject) — revisiting an already-empty shoot
    // won't re-trigger because no flag mutation happens there.
    const triageJustEmptied =
      currentView === "triage" &&
      newDisplayItems.length === 0 &&
      oldFlag === "unreviewed" &&
      (flag === "pick" || flag === "reject");
    if (triageJustEmptied) {
      void get().setView("select");
      try {
        await invoke("set_flag", { photoId: image.id, flag });
      } catch (e) {
        console.error("Failed to set flag:", e);
        get().setToast(`Flag save failed: ${e}`, "error");
      }
      return;
    }

    // Auto-exit just happened if we were drilled in and the helper
    // returned null. In that case, jump to where the old group sat in
    // the outer list — post-removal, that index holds whatever came
    // next (natural continue-where-you-left-off). Otherwise keep the
    // old clamp-to-range behavior.
    const autoExited = preActiveId != null && newActive == null;
    const advanceTarget = autoExited && preDrilledOuterIdx >= 0
      ? Math.min(preDrilledOuterIdx, Math.max(0, newDisplayItems.length - 1))
      : Math.min(currentIndex, Math.max(0, newDisplayItems.length - 1));

    const maybeDrillIn = () => {
      const items = get().displayItems;
      const target = items[advanceTarget];
      if (
        target?.isGroupCover &&
        target.groupId != null &&
        (currentView === "triage" || currentView === "select")
      ) {
        get().setActiveInnerGroup(target.groupId);
      }
    };

    if (autoAdvance) {
      setTimeout(() => {
        set({ currentIndex: advanceTarget, isZoomed: false });
        maybeDrillIn();
      }, 150);
    } else {
      if (advanceTarget !== currentIndex) {
        set({ currentIndex: advanceTarget });
      }
      maybeDrillIn();
    }

    try {
      await invoke("set_flag", { photoId: image.id, flag });
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

  bulkSetDestination: async (photoIds: number[], dest: string) => {
    const { images, currentView, groups, undoStack } = get();
    if (photoIds.length === 0) return;

    // Snapshot the old destinations so undo can restore each photo
    // individually — per-photo oldValue means a bulk Ctrl+Z reverts to
    // the exact prior state, not a uniform one.
    const batch: NonNullable<UndoEntry["batch"]> = [];
    const updatedImages = [...images];
    for (const id of photoIds) {
      const idx = updatedImages.findIndex((i) => i.id === id);
      if (idx < 0) continue;
      const oldDest = updatedImages[idx].destination;
      if (oldDest === dest) continue;
      batch.push({
        imageId: id,
        field: "destination",
        oldValue: oldDest,
        newValue: dest,
      });
      updatedImages[idx] = { ...updatedImages[idx], destination: dest };
    }
    if (batch.length === 0) return;

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
          imageId: batch[0].imageId,
          field: "destination",
          oldValue: batch[0].oldValue,
          newValue: dest,
          batch,
        },
      ],
      redoStack: [],
    });

    try {
      for (const entry of batch) {
        await invoke("set_destination", {
          photoId: entry.imageId,
          destination: dest,
        });
      }
    } catch (e) {
      console.error("Bulk destination failed:", e);
      get().setToast(`Some destinations failed: ${e}`, "error");
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
      ? entry.batch.map((b) => ({
          imageId: b.imageId,
          value: b.oldValue,
          field: b.field ?? entry.field,
        }))
      : [{ imageId: entry.imageId, value: entry.oldValue, field: entry.field }];

    for (const t of targets) {
      const idx = updatedImages.findIndex((img) => img.id === t.imageId);
      if (idx < 0) continue;
      if (t.field === "starRating") {
        updatedImages[idx] = { ...updatedImages[idx], starRating: t.value as number };
      } else if (t.field === "flag") {
        updatedImages[idx] = { ...updatedImages[idx], flag: t.value as string };
      } else if (t.field === "destination") {
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
        if (t.field === "flag") {
          await invoke("set_flag", { photoId: t.imageId, flag: t.value });
        } else if (t.field === "destination") {
          await invoke("set_destination", { photoId: t.imageId, destination: t.value });
        } else if (t.field === "starRating") {
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
      ? entry.batch.map((b) => ({
          imageId: b.imageId,
          value: b.newValue,
          field: b.field ?? entry.field,
        }))
      : [{ imageId: entry.imageId, value: entry.newValue, field: entry.field }];

    for (const t of targets) {
      const idx = updatedImages.findIndex((img) => img.id === t.imageId);
      if (idx < 0) continue;
      if (t.field === "starRating") {
        updatedImages[idx] = { ...updatedImages[idx], starRating: t.value as number };
      } else if (t.field === "flag") {
        updatedImages[idx] = { ...updatedImages[idx], flag: t.value as string };
      } else if (t.field === "destination") {
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
        if (t.field === "flag") {
          await invoke("set_flag", { photoId: t.imageId, flag: t.value });
        } else if (t.field === "destination") {
          await invoke("set_destination", { photoId: t.imageId, destination: t.value });
        } else if (t.field === "starRating") {
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
    const fromView = get().currentView;
    if (currentPhotoId !== undefined) {
      invoke("set_view_cursor", {
        shootId: currentShoot.id,
        viewName: fromView,
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
    });
    get().autoDrillIfOnCover();

    // Auto-reorganize on pass completion. Runs AFTER the view has
    // already switched so the sync's IPC round-trip doesn't delay the
    // UI transition. When files actually move, we reload images and
    // recompute displayItems so tooltips/metadata reflect the new
    // paths — but the user sees the view change immediately.
    const trigger = resolveSyncTrigger(fromView, view);
    if (trigger) {
      invoke<SyncReport | null>("sync_layout_if_eligible", {
        shootId: currentShoot.id,
        trigger,
      })
        .then(async (report) => {
          if (!report || report.moved.length === 0) return;
          const fresh = await invoke<ImageEntry[]>("get_image_list");
          set({ images: fresh });
          get().refreshDisplay();
          get().setToast(
            `Moved ${report.moved.length} file${report.moved.length === 1 ? "" : "s"}`,
          );
        })
        .catch((e) => {
          console.error("sync_layout_if_eligible failed", e);
        });
    }
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

  toggleShortcutHints: () =>
    set((s) => ({ showShortcutHints: !s.showShortcutHints })),
  toggleAiPanel: () => set((s) => ({ aiPanelForced: !s.aiPanelForced })),
  toggleAllStrip: () =>
    set((s) => {
      const next = !s.showAllStrip;
      writeBoolLS("photosift.rail.allStrip", next);
      return { showAllStrip: next };
    }),
  toggleFaces: () =>
    set((s) => {
      const next = !s.showFaces;
      writeBoolLS("photosift.rail.faces", next);
      return { showFaces: next };
    }),

  keepAllInCurrentGroup: async () => {
    const { displayItems, currentIndex, images, groups, currentView } = get();
    if (currentView !== "triage") return;
    const item = displayItems[currentIndex];
    if (!item) return;
    const gid = item.groupId
      ?? groups.find((g) => g.members.some((m) => m.photoId === item.image.id))?.id;
    if (gid === undefined) return;
    const group = groups.find((g) => g.id === gid);
    if (!group) return;

    const targets: { id: number; oldFlag: string }[] = [];
    for (const member of group.members) {
      const img = images.find((i) => i.id === member.photoId);
      if (img && img.flag !== "pick") {
        targets.push({ id: img.id, oldFlag: img.flag });
      }
    }
    if (targets.length === 0) return;

    const targetIds = new Set(targets.map((t) => t.id));
    const updatedImages = images.map((img) =>
      targetIds.has(img.id) ? { ...img, flag: "pick" } : img,
    );

    try {
      await invoke("bulk_set_flag", {
        photoIds: targets.map((t) => t.id),
        flag: "pick",
      });
    } catch (err) {
      get().setToast(`Keep-group failed: ${err}`, "error");
      return;
    }

    const { items: newDisplayItems, activeInnerGroupId: newActive } =
      computeWithAutoExit(
        updatedImages,
        currentView,
        groups,
        get().activeInnerGroupId,
        currentAiOptions(get().sortByAi),
      );

    const [first, ...rest] = targets;
    const undoEntry: UndoEntry = {
      imageId: first.id,
      field: "flag",
      oldValue: first.oldFlag,
      newValue: "pick",
      batch: [first, ...rest].map((t) => ({
        imageId: t.id,
        oldValue: t.oldFlag,
        newValue: "pick",
      })),
    };

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      activeInnerGroupId: newActive,
      undoStack: [...get().undoStack.slice(-49), undoEntry],
      redoStack: [],
      lastFlagAction: { color: "rgba(34, 197, 94, 0.15)", timestamp: Date.now() },
    });
    get().setToast(`Kept ${targets.length} in group`);
  },
  toggleHeatmap: () => set((s) => ({ heatmapOn: !s.heatmapOn })),
  toggleShowReviewed: () => {
    set((s) => ({ showReviewed: !s.showReviewed }));
    get().refreshDisplay();
  },

  setSelectMinStar: (n: number) => {
    const clamped = Math.max(0, Math.min(5, n));
    if (get().selectMinStar === clamped) return;
    // Moving between tiers (manual `[` / `]` or auto-bump) resets the
    // visited-at-floor tracking so each pass starts fresh. Also drop
    // any active bracket — a tier change invalidates pairings.
    set({
      selectMinStar: clamped,
      selectVisitedAtFloor: new Set<number>(),
      selectBracket: null,
      selectBracketSuppressedForGroup: null,
    });
    get().refreshDisplay();
    const shootId = get().currentShoot?.id;
    if (shootId != null && clamped >= 1) {
      invoke("bump_select_max_floor", { shootId, floor: clamped }).catch(() => {});
    }
  },

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

    // When drilling INTO a group, snap focus to the top-ranked member
    // (displayItems[0], since the drill-down sorts by quality desc).
    // Using the clicked cover's photoId as a hint kept the user sitting
    // on whatever happened to be the group cover — often a mediocre
    // frame — forcing an extra arrow-key press before P/X on the best
    // shot. When drilling OUT (groupId === null) we preserve the
    // currentPhotoId hint so the user lands back where they were.
    let newIndex = 0;
    if (nextActive === null && currentPhotoId !== undefined) {
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

  autoDrillIfOnCover: () => {
    const { currentView, activeInnerGroupId, displayItems, currentIndex } = get();
    if (activeInnerGroupId != null) return;
    if (currentView !== "triage" && currentView !== "select") return;
    const target = displayItems[currentIndex];
    if (target?.isGroupCover && target.groupId != null) {
      get().setActiveInnerGroup(target.groupId);
    }
  },

  setFlagNoAutoReject: async (flag: string) => {
    const { displayItems, currentIndex, autoAdvance, undoStack, images, currentView, groups } = get();
    const item = displayItems[currentIndex];
    if (!item) return;

    const image = item.image;
    const oldFlag = image.flag;
    if (oldFlag === flag) return;

    const preActiveId = get().activeInnerGroupId;
    const preDrilledOuterIdx =
      preActiveId != null
        ? outerIndexOfGroupCover(
            images,
            currentView,
            groups,
            preActiveId,
            currentAiOptions(get().sortByAi),
          )
        : -1;

    const updatedImages = [...images];
    updatedImages[item.imageIndex] = { ...image, flag };
    const { items: newDisplayItems, activeInnerGroupId: newActive } =
      computeWithAutoExit(
        updatedImages,
        currentView,
        groups,
        get().activeInnerGroupId,
        currentAiOptions(get().sortByAi),
      );

    const flashColor = flag === "pick" ? "rgba(34, 197, 94, 0.15)" : flag === "reject" ? "rgba(239, 68, 68, 0.15)" : null;

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      activeInnerGroupId: newActive,
      undoStack: [...undoStack.slice(-49), { imageId: image.id, field: "flag", oldValue: oldFlag, newValue: flag }],
      redoStack: [],
      lastFlagAction: flashColor ? { color: flashColor, timestamp: Date.now() } : get().lastFlagAction,
    });

    const triageJustEmptied =
      currentView === "triage" &&
      newDisplayItems.length === 0 &&
      oldFlag === "unreviewed" &&
      (flag === "pick" || flag === "reject");
    if (triageJustEmptied) {
      void get().setView("select");
      try {
        await invoke("set_flag", { photoId: image.id, flag });
      } catch (e) {
        console.error("Failed to set flag:", e);
        get().setToast(`Flag save failed: ${e}`, "error");
      }
      return;
    }

    const autoExited = preActiveId != null && newActive == null;
    const advanceTarget = autoExited && preDrilledOuterIdx >= 0
      ? Math.min(preDrilledOuterIdx, Math.max(0, newDisplayItems.length - 1))
      : Math.min(currentIndex, Math.max(0, newDisplayItems.length - 1));

    const maybeDrillIn = () => {
      const items = get().displayItems;
      const target = items[advanceTarget];
      if (
        target?.isGroupCover &&
        target.groupId != null &&
        (currentView === "triage" || currentView === "select")
      ) {
        get().setActiveInnerGroup(target.groupId);
      }
    };

    if (autoAdvance) {
      setTimeout(() => {
        set({ currentIndex: advanceTarget, isZoomed: false });
        maybeDrillIn();
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

  enterBracket: () => {
    const {
      currentView,
      displayItems,
      currentIndex,
      groups,
      images,
      selectBracket,
    } = get();
    if (currentView !== "select") return;
    if (selectBracket && !selectBracket.isComplete) return;
    const item = displayItems[currentIndex];
    if (!item?.groupId) return;

    const group = groups.find((g) => g.id === item.groupId);
    if (!group) return;

    const members = group.members
      .map((m) => images.find((i) => i.id === m.photoId))
      .filter((i): i is ImageEntry => !!i && i.flag !== "reject")
      .map((i) => ({ id: i.id, qualityScore: i.qualityScore ?? null }));

    if (members.length < 2) return;

    // Explicit enter (keyboard Tab or direct call) clears any prior
    // suppression for this group. Auto-enter (SelectShell effect) is
    // gated separately by checking suppressedForGroup at the call site.
    set({
      selectBracket: createBracket(group.id, members),
      selectBracketSuppressedForGroup: null,
    });
  },

  exitBracket: () => {
    const { selectBracket } = get();
    const suppressGroupId = selectBracket?.groupId ?? null;
    set({
      selectBracket: null,
      selectBracketSuppressedForGroup: suppressGroupId,
    });
  },

  bracketDecision: async (decision: Decision) => {
    const { selectBracket, images } = get();
    if (!selectBracket || selectBracket.isComplete) return;
    const pair = currentPair(selectBracket);
    if (!pair || pair.right === null) return;

    const nextBracket = applyDecision(selectBracket, decision);
    const promoted = nextBracket.lastPromoted;

    // Apply +1 star to each promoted photo. Clamp at 5. Single batch
    // undo entry so one Ctrl+Z reverts the whole decision.
    const updatedImages = [...images];
    const batchEntries: NonNullable<UndoEntry["batch"]> = [];
    for (const pid of promoted) {
      const idx = updatedImages.findIndex((i) => i.id === pid);
      if (idx < 0) continue;
      const oldRating = updatedImages[idx].starRating;
      const newRating = Math.min(5, oldRating + 1);
      if (newRating === oldRating) continue;
      updatedImages[idx] = { ...updatedImages[idx], starRating: newRating };
      batchEntries.push({
        imageId: pid,
        field: "starRating",
        oldValue: oldRating,
        newValue: newRating,
      });
    }

    const newDisplayItems = computeDisplayItemsFiltered(
      updatedImages,
      get().currentView,
      get().groups,
      get().activeInnerGroupId,
      selectRequiresPick(),
      routeMinStar(),
      currentAiOptions(get().sortByAi),
    );

    const undoPatch: Partial<ProjectState> = {};
    if (batchEntries.length > 0) {
      undoPatch.undoStack = [
        ...get().undoStack.slice(-49),
        {
          imageId: batchEntries[0].imageId,
          field: "starRating",
          oldValue: batchEntries[0].oldValue,
          newValue: batchEntries[0].newValue,
          batch: batchEntries,
        },
      ];
      undoPatch.redoStack = [];
    }

    // Mark every member of the pair visited-at-floor regardless of
    // decision — we've now "seen" them for this pass.
    const visited = new Set(get().selectVisitedAtFloor);
    visited.add(pair.left);
    if (pair.right !== null) visited.add(pair.right);

    set({
      images: updatedImages,
      displayItems: newDisplayItems,
      selectBracket: nextBracket,
      selectVisitedAtFloor: visited,
      ...undoPatch,
    });

    // If the bracket is now complete, mark every original seed as
    // visited (they've all been reviewed in this group) and drop the
    // bracket state. Advance the cursor past the current group so the
    // user lands on the next item in the flat displayItems list. If
    // we're drilled in, step out first — otherwise "past the group" is
    // past the filtered member list, which dead-ends.
    if (nextBracket.isComplete) {
      const fullyVisited = new Set(get().selectVisitedAtFloor);
      for (const pid of nextBracket.seedOrder) fullyVisited.add(pid);
      set({
        selectBracket: null,
        selectVisitedAtFloor: fullyVisited,
      });
      if (get().activeInnerGroupId !== null) {
        get().setActiveInnerGroup(null);
      }
      const finishedGroupId = nextBracket.groupId;
      const freshDisplay = get().displayItems;
      const cursor = get().currentIndex;
      let nextCursor = cursor;
      while (
        nextCursor < freshDisplay.length &&
        freshDisplay[nextCursor].groupId === finishedGroupId
      ) {
        nextCursor++;
      }
      const target = nextCursor >= freshDisplay.length ? cursor : nextCursor;
      if (target !== cursor) {
        set({ currentIndex: target });
      }
    }

    try {
      for (const entry of batchEntries) {
        await invoke("set_rating", {
          imageId: entry.imageId,
          rating: entry.newValue,
        });
      }
    } catch (e) {
      console.error("Failed bracket decision:", e);
      get().setToast(`Bracket save failed: ${e}`, "error");
    }

    // After the bracket completed and we advance out of the group,
    // check if this tier is done.
    if (nextBracket.isComplete) {
      get().maybeBumpFloor();
    }
  },

  pickCurrent: async () => {
    const { displayItems, currentIndex } = get();
    const item = displayItems[currentIndex];
    if (!item) return;
    const photoId = item.image.id;
    const current = item.image.starRating;
    const next = Math.min(5, current + 1);
    if (next !== current) {
      await get().setRating(next);
    } else {
      // Already capped; still count as visited + advance.
      get().markVisitedAtFloor(photoId);
      get().navigateNext();
    }
  },

  skipCurrent: () => {
    const { displayItems, currentIndex } = get();
    const item = displayItems[currentIndex];
    if (item) get().markVisitedAtFloor(item.image.id);
    get().navigateNext();
  },

  markVisitedAtFloor: (photoId: number) => {
    const { currentView, selectVisitedAtFloor } = get();
    if (currentView !== "select") return;
    if (selectVisitedAtFloor.has(photoId)) {
      get().maybeBumpFloor();
      return;
    }
    const next = new Set(selectVisitedAtFloor);
    next.add(photoId);
    set({ selectVisitedAtFloor: next });
    get().maybeBumpFloor();
  },

  /// Check if every visible displayItem has been visited at the current
  /// floor. If so, bump the floor by 1 and reset the cursor. Internal
  /// helper, invoked by markVisitedAtFloor and setRating.
  maybeBumpFloor: () => {
    const { currentView, displayItems, selectMinStar, selectVisitedAtFloor } = get();
    if (currentView !== "select") return;
    if (selectMinStar >= 5) return;
    if (displayItems.length === 0) return;
    const allVisited = displayItems.every((d) => selectVisitedAtFloor.has(d.image.id));
    if (!allVisited) return;
    // Bump and reset.
    get().setSelectMinStar(selectMinStar + 1);
    set({ currentIndex: 0, isZoomed: false });
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
    const { items: next, activeInnerGroupId: nextActive } = computeWithAutoExit(
      images,
      currentView,
      groups,
      get().activeInnerGroupId,
      currentAiOptions(get().sortByAi),
    );
    let nextIndex = currentIndex;
    if (currentPhotoId !== undefined) {
      const idx = next.findIndex((d) => d.image.id === currentPhotoId);
      if (idx >= 0) nextIndex = idx;
    }
    nextIndex = Math.min(nextIndex, Math.max(0, next.length - 1));
    set({
      displayItems: next,
      activeInnerGroupId: nextActive,
      currentIndex: nextIndex < 0 ? 0 : nextIndex,
    });
    get().autoDrillIfOnCover();
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
        qualityScore: fresh.qualityScore,
        maxSmileScore: fresh.maxSmileScore,
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
// via the `selectRequiresPick()` / `routeMinStar()` helpers above — but
// updating a setting in SettingsDialog only mutates `settingsStore`; it
// doesn't trigger a re-compute here. Subscribe so the user sees the
// filter / sort take effect as soon as they adjust the slider.
let lastSelectRequiresPick = useSettingsStore.getState().settings.selectRequiresPick;
let lastRouteMinStar = useSettingsStore.getState().settings.routeMinStar;
useSettingsStore.subscribe((state) => {
  const { selectRequiresPick: sr, routeMinStar: rm } = state.settings;
  if (sr !== lastSelectRequiresPick || rm !== lastRouteMinStar) {
    lastSelectRequiresPick = sr;
    lastRouteMinStar = rm;
    useProjectStore.getState().refreshDisplay();
  }
});
