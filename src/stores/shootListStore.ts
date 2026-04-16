import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ShootSummary } from "../types";

interface ShootListState {
  shoots: ShootSummary[];
  isLoading: boolean;
  refresh: () => Promise<void>;
}

export const useShootListStore = create<ShootListState>((set) => ({
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
}));
