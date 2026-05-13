import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ShootSummary } from "../types";

interface ShootListState {
  shoots: ShootSummary[];
  isLoading: boolean;
  /// Set when the most recent `refresh()` failed. Non-null with an empty
  /// `shoots` list means "couldn't load" (distinct from "no shoots yet").
  loadError: string | null;
  refresh: () => Promise<void>;
  deleteShoot: (shootId: number) => Promise<void>;
}

export const useShootListStore = create<ShootListState>((set, get) => ({
  shoots: [],
  isLoading: false,
  loadError: null,

  refresh: async () => {
    set({ isLoading: true, loadError: null });
    try {
      const shoots = await invoke<ShootSummary[]>("list_shoots");
      set({ shoots, isLoading: false, loadError: null });
    } catch (e) {
      console.error("Failed to list shoots:", e);
      set({ isLoading: false, loadError: String(e) });
    }
  },

  deleteShoot: async (shootId: number) => {
    await invoke("delete_shoot", { shootId });
    // Optimistically prune, then re-list to stay consistent with backend.
    set({ shoots: get().shoots.filter((s) => s.id !== shootId) });
    await get().refresh();
  },
}));
