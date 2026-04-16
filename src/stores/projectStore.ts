import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ImageEntry, ShootSummary } from "../types";

interface UndoEntry {
  imageId: number;
  field: "starRating" | "flag" | "destination";
  oldValue: string | number;
  newValue: string | number;
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

  loadShoot: async (shootId: number) => {
    set({ isLoading: true });
    try {
      const shoot = await invoke<ShootSummary>("get_shoot", { shootId });
      const images = await invoke<ImageEntry[]>("get_image_list");
      set({
        currentShoot: shoot,
        images,
        currentIndex: 0,
        isLoading: false,
        undoStack: [],
        redoStack: [],
      });
    } catch (e) {
      console.error("Failed to load shoot:", e);
      set({ isLoading: false });
    }
  },

  setCurrentIndex: (index: number) => {
    const { images } = get();
    if (index >= 0 && index < images.length) {
      set({ currentIndex: index, isZoomed: false });
    }
  },

  navigateNext: () => {
    const { currentIndex, images } = get();
    if (currentIndex < images.length - 1) {
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
    const { images, currentIndex, autoAdvance, undoStack } = get();
    const image = images[currentIndex];
    if (!image) return;

    const oldRating = image.starRating;
    if (oldRating === rating) return;

    const updatedImages = [...images];
    updatedImages[currentIndex] = { ...image, starRating: rating };
    set({
      images: updatedImages,
      undoStack: [
        ...undoStack.slice(-49),
        { imageId: image.id, field: "starRating", oldValue: oldRating, newValue: rating },
      ],
      redoStack: [],
    });

    if (autoAdvance && currentIndex < images.length - 1) {
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
        set({ images: revertImages });
      }
    }
  },

  setFlag: async (flag: string) => {
    const { images, currentIndex, autoAdvance, undoStack } = get();
    const image = images[currentIndex];
    if (!image) return;

    const oldFlag = image.flag;
    if (oldFlag === flag) return;

    const updatedImages = [...images];
    updatedImages[currentIndex] = { ...image, flag };
    set({
      images: updatedImages,
      undoStack: [
        ...undoStack.slice(-49),
        { imageId: image.id, field: "flag", oldValue: oldFlag, newValue: flag },
      ],
      redoStack: [],
    });

    if (autoAdvance && currentIndex < images.length - 1) {
      set({ currentIndex: currentIndex + 1, isZoomed: false });
    }

    try {
      await invoke("set_flag", { photoId: image.id, flag });
    } catch (e) {
      console.error("Failed to set flag:", e);
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], flag: oldFlag };
        set({ images: revertImages });
      }
    }
  },

  setDestination: async (dest: string) => {
    const { images, currentIndex, undoStack } = get();
    const image = images[currentIndex];
    if (!image) return;

    const oldDest = image.destination;
    if (oldDest === dest) return;

    const updatedImages = [...images];
    updatedImages[currentIndex] = { ...image, destination: dest };
    set({
      images: updatedImages,
      undoStack: [
        ...undoStack.slice(-49),
        { imageId: image.id, field: "destination", oldValue: oldDest, newValue: dest },
      ],
      redoStack: [],
    });

    try {
      await invoke("set_destination", { photoId: image.id, destination: dest });
    } catch (e) {
      console.error("Failed to set destination:", e);
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], destination: oldDest };
        set({ images: revertImages });
      }
    }
  },

  undo: async () => {
    const { undoStack, redoStack, images } = get();
    const entry = undoStack[undoStack.length - 1];
    if (!entry) return;

    const idx = images.findIndex((img) => img.id === entry.imageId);
    if (idx < 0) return;

    const updatedImages = [...images];
    if (entry.field === "starRating") {
      updatedImages[idx] = { ...updatedImages[idx], starRating: entry.oldValue as number };
    } else if (entry.field === "flag") {
      updatedImages[idx] = { ...updatedImages[idx], flag: entry.oldValue as string };
    } else if (entry.field === "destination") {
      updatedImages[idx] = { ...updatedImages[idx], destination: entry.oldValue as string };
    }

    set({
      images: updatedImages,
      undoStack: undoStack.slice(0, -1),
      redoStack: [...redoStack, entry],
      currentIndex: idx,
    });

    try {
      await invoke("undo_last");
    } catch (e) {
      console.error("Undo failed:", e);
    }
  },

  redo: async () => {
    const { redoStack, undoStack, images } = get();
    const entry = redoStack[redoStack.length - 1];
    if (!entry) return;

    const idx = images.findIndex((img) => img.id === entry.imageId);
    if (idx < 0) return;

    const updatedImages = [...images];
    if (entry.field === "starRating") {
      updatedImages[idx] = { ...updatedImages[idx], starRating: entry.newValue as number };
      await invoke("set_rating", { imageId: entry.imageId, rating: entry.newValue }).catch(() => {});
    } else if (entry.field === "flag") {
      updatedImages[idx] = { ...updatedImages[idx], flag: entry.newValue as string };
      await invoke("set_flag", { photoId: entry.imageId, flag: entry.newValue }).catch(() => {});
    } else if (entry.field === "destination") {
      updatedImages[idx] = { ...updatedImages[idx], destination: entry.newValue as string };
      await invoke("set_destination", { photoId: entry.imageId, destination: entry.newValue }).catch(() => {});
    }

    set({
      images: updatedImages,
      redoStack: redoStack.slice(0, -1),
      undoStack: [...undoStack, entry],
      currentIndex: idx,
    });
  },

  toggleMetadata: () => set((s) => ({ showMetadata: !s.showMetadata })),
  toggleShortcutHints: () => set((s) => ({ showShortcutHints: !s.showShortcutHints })),
  toggleAutoAdvance: () => set((s) => ({ autoAdvance: !s.autoAdvance })),
  toggleZoom: () => set((s) => ({ isZoomed: !s.isZoomed })),
}));
