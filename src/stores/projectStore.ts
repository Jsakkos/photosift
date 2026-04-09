import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ImageEntry, ProjectInfo } from "../types";

interface UndoEntry {
  imageId: number;
  field: "starRating";
  oldValue: number;
  newValue: number;
}

interface ProjectState {
  projectInfo: ProjectInfo | null;
  images: ImageEntry[];
  currentIndex: number;
  isLoading: boolean;
  showMetadata: boolean;
  showShortcutHints: boolean;
  autoAdvance: boolean;
  isZoomed: boolean;
  undoStack: UndoEntry[];
  redoStack: UndoEntry[];

  openProject: (folderPath: string) => Promise<void>;
  setCurrentIndex: (index: number) => void;
  navigateNext: () => void;
  navigatePrev: () => void;
  setRating: (rating: number) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  toggleMetadata: () => void;
  toggleShortcutHints: () => void;
  toggleAutoAdvance: () => void;
  toggleZoom: () => void;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  projectInfo: null,
  images: [],
  currentIndex: 0,
  isLoading: false,
  showMetadata: false,
  showShortcutHints: false,
  autoAdvance: true,
  isZoomed: false,
  undoStack: [],
  redoStack: [],

  openProject: async (folderPath: string) => {
    set({ isLoading: true });
    try {
      const info = await invoke<ProjectInfo>("open_project", { folderPath });
      const images = await invoke<ImageEntry[]>("get_image_list");
      set({
        projectInfo: info,
        images,
        currentIndex: info.lastViewedIndex,
        isLoading: false,
        undoStack: [],
        redoStack: [],
      });
    } catch (e) {
      console.error("Failed to open project:", e);
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

  undo: async () => {
    const { undoStack, redoStack, images } = get();
    const entry = undoStack[undoStack.length - 1];
    if (!entry) return;

    const idx = images.findIndex((img) => img.id === entry.imageId);
    if (idx < 0) return;

    const updatedImages = [...images];
    updatedImages[idx] = { ...updatedImages[idx], starRating: entry.oldValue };

    set({
      images: updatedImages,
      undoStack: undoStack.slice(0, -1),
      redoStack: [...redoStack, entry],
      currentIndex: idx,
    });

    try {
      await invoke("set_rating", { imageId: entry.imageId, rating: entry.oldValue });
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
    updatedImages[idx] = { ...updatedImages[idx], starRating: entry.newValue };

    set({
      images: updatedImages,
      redoStack: redoStack.slice(0, -1),
      undoStack: [...undoStack, entry],
      currentIndex: idx,
    });

    try {
      await invoke("set_rating", { imageId: entry.imageId, rating: entry.newValue });
    } catch (e) {
      console.error("Redo failed:", e);
    }
  },

  toggleMetadata: () => set((s) => ({ showMetadata: !s.showMetadata })),
  toggleShortcutHints: () => set((s) => ({ showShortcutHints: !s.showShortcutHints })),
  toggleAutoAdvance: () => set((s) => ({ autoAdvance: !s.autoAdvance })),
  toggleZoom: () => set((s) => ({ isZoomed: !s.isZoomed })),
}));
