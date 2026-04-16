import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  nearDupThreshold: number;
  relatedThreshold: number;
  triageExpandGroups: boolean;
}

const DEFAULT_SETTINGS: Settings = {
  nearDupThreshold: 4,
  relatedThreshold: 12,
  triageExpandGroups: false,
};

interface SettingsState {
  settings: Settings;
  isLoaded: boolean;
  isOpen: boolean;
  loadSettings: () => Promise<void>;
  updateSettings: (partial: Partial<Settings>) => Promise<void>;
  reclusterShoot: (shootId: number) => Promise<number>;
  openDialog: () => void;
  closeDialog: () => void;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  isLoaded: false,
  isOpen: false,

  loadSettings: async () => {
    try {
      const s = await invoke<Settings>("get_settings");
      set({ settings: s, isLoaded: true });
    } catch (e) {
      console.error("Failed to load settings:", e);
      set({ settings: DEFAULT_SETTINGS, isLoaded: true });
    }
  },

  updateSettings: async (partial: Partial<Settings>) => {
    const next = { ...get().settings, ...partial };
    set({ settings: next });
    try {
      await invoke("update_settings", { settings: next });
    } catch (e) {
      console.error("Failed to persist settings:", e);
    }
  },

  reclusterShoot: async (shootId: number) => {
    return await invoke<number>("recluster_shoot", { shootId });
  },

  openDialog: () => set({ isOpen: true }),
  closeDialog: () => set({ isOpen: false }),
}));
