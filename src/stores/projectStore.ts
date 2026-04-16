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

export function computeDisplayItems(
  images: ImageEntry[],
  currentView: CullView,
  groups: Group[],
): DisplayItem[] {
  const items: DisplayItem[] = [];
  const photoGroupMap = buildPhotoGroupMap(groups);

  if (currentView === "triage") {
    const seenGroups = new Set<number>();
    for (let i = 0; i < images.length; i++) {
      const img = images[i];
      if (img.flag !== "unreviewed") continue;

      const group = photoGroupMap.get(img.id);
      if (group) {
        if (seenGroups.has(group.id)) continue;
        seenGroups.add(group.id);
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
    for (let i = 0; i < images.length; i++) {
      const img = images[i];
      if (img.flag === "reject") continue;

      const group = photoGroupMap.get(img.id);
      if (group) {
        if (seenGroups.has(group.id)) continue;
        seenGroups.add(group.id);
        for (const member of group.members) {
          const memberIdx = images.findIndex((im) => im.id === member.photoId);
          if (memberIdx < 0) continue;
          const memberImg = images[memberIdx];
          if (memberImg.flag === "reject") continue;
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
      if (img.flag === "pick" && img.destination === "unrouted") {
        items.push({ imageIndex: i, image: img });
      }
    }
  }

  return items;
}

interface ProjectState {
  currentShoot: ShootSummary | null;
  images: ImageEntry[];
  currentIndex: number;
  isLoading: boolean;
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
  lastFlagAction: { color: string; timestamp: number } | null;
  toast: { message: string; kind: "info" | "error"; timestamp: number } | null;

  loadShoot: (shootId: number) => Promise<void>;
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
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  currentShoot: null,
  images: [],
  currentIndex: 0,
  isLoading: false,
  showMetadata: false,
  showShortcutHints: false,
  autoAdvance: true,
  isZoomed: false,
  undoStack: [],
  redoStack: [],
  currentView: "triage",
  viewMode: "sequential",
  groups: [],
  displayItems: [],
  lastFlagAction: null,
  toast: null,
  comparisonPinnedId: null,
  comparisonCyclingId: null,
  comparisonGroupMembers: [],

  currentImage: () => {
    const { displayItems, currentIndex } = get();
    return displayItems[currentIndex]?.image ?? null;
  },

  loadShoot: async (shootId: number) => {
    set({ isLoading: true });
    try {
      const shoot = await invoke<ShootSummary>("get_shoot", { shootId });
      const images = await invoke<ImageEntry[]>("get_image_list");

      const groups = await invoke<Group[]>("get_groups_for_shoot", {
        shootId,
      }).catch(() => [] as Group[]);

      const cursor = await invoke<number | null>("get_view_cursor", {
        shootId,
        viewName: "triage",
      }).catch(() => null);

      const displayItems = computeDisplayItems(images, "triage", groups);

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
        currentView: "triage",
        viewMode: "sequential",
        groups,
        displayItems,
        lastFlagAction: null,
      });
    } catch (e) {
      console.error("Failed to load shoot:", e);
      set({ isLoading: false });
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
    const newDisplayItems = computeDisplayItems(
      updatedImages,
      get().currentView,
      get().groups,
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
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], starRating: oldRating };
        set({
          images: revertImages,
          displayItems: computeDisplayItems(
            revertImages,
            get().currentView,
            get().groups,
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
          invoke("bulk_set_flag", { photoIds: siblingIds, flag: "reject" }).catch(() => {});
        }
      }
    } else {
      updatedImages[item.imageIndex] = { ...image, flag };
      affectedIds.push({ id: image.id, oldFlag });
    }

    const newDisplayItems = computeDisplayItems(updatedImages, currentView, groups);

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
      const revertImages = [...get().images];
      for (const a of affectedIds) {
        const idx = revertImages.findIndex((img) => img.id === a.id);
        if (idx >= 0) {
          revertImages[idx] = { ...revertImages[idx], flag: a.oldFlag };
        }
      }
      set({
        images: revertImages,
        displayItems: computeDisplayItems(
          revertImages,
          get().currentView,
          get().groups,
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
    const newDisplayItems = computeDisplayItems(
      updatedImages,
      currentView,
      groups,
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
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], destination: oldDest };
        set({
          images: revertImages,
          displayItems: computeDisplayItems(
            revertImages,
            get().currentView,
            get().groups,
          ),
        });
      }
    }
  },

  undo: async () => {
    const { undoStack, redoStack, images, currentView, groups } = get();
    const entry = undoStack[undoStack.length - 1];
    if (!entry) return;

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

    const newDisplayItems = computeDisplayItems(
      updatedImages,
      currentView,
      groups,
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
    } catch (e) {
      console.error("Undo failed:", e);
    }
  },

  redo: async () => {
    const { redoStack, undoStack, images, currentView, groups } = get();
    const entry = redoStack[redoStack.length - 1];
    if (!entry) return;

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

    const newDisplayItems = computeDisplayItems(
      updatedImages,
      currentView,
      groups,
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
    } catch (e) {
      console.error("Redo failed:", e);
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

    const newDisplayItems = computeDisplayItems(images, view, groups);

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
  toggleAutoAdvance: () => set((s) => ({ autoAdvance: !s.autoAdvance })),
  toggleZoom: () => set((s) => ({ isZoomed: !s.isZoomed })),

  setFlagNoAutoReject: async (flag: string) => {
    const { displayItems, currentIndex, autoAdvance, undoStack, images, currentView, groups } = get();
    const item = displayItems[currentIndex];
    if (!item) return;

    const image = item.image;
    const oldFlag = image.flag;
    if (oldFlag === flag) return;

    const updatedImages = [...images];
    updatedImages[item.imageIndex] = { ...image, flag };
    const newDisplayItems = computeDisplayItems(updatedImages, currentView, groups);

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
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], flag: oldFlag };
        set({ images: revertImages, displayItems: computeDisplayItems(revertImages, get().currentView, get().groups) });
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
      set({ groups });
    }
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

    const newDisplayItems = computeDisplayItems(updatedImages, currentView, groups);

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
        displayItems: computeDisplayItems(images, currentView, groups),
      });
    } catch (e) {
      console.error("Create group failed:", e);
      get().setToast(`Group failed: ${e}`, "error");
    }
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
        displayItems: computeDisplayItems(images, currentView, groups),
      });
    } catch (e) {
      console.error("Ungroup failed:", e);
      get().setToast(`Ungroup failed: ${e}`, "error");
    }
  },
}));
