import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ShootSummary } from "../types";

interface ShootListState {
  shoots: ShootSummary[];
  isLoading: boolean;
  refresh: () => Promise<void>;
  deleteShoot: (shootId: number) => Promise<void>;
}

export const useShootListStore = create<ShootListState>((set, get) => ({
  shoots: [],
  isLoading: false,

  refresh: async () => {
    set({ isLoading: true });
    try {
      const shoots = await invoke<ShootSummary[]>("list_shoots");
      set({ shoots, isLoading: false });
    } catch (e) {
      console.error("Failed to list shoots:", e);
      set({ isLoading: false });
    }
  },

  deleteShoot: async (shootId: number) => {
    await invoke("delete_shoot", { shootId });
    // Optimistically prune, then re-list to stay consistent with backend.
    set({ shoots: get().shoots.filter((s) => s.id !== shootId) });
    await get().refresh();
  },
}));
