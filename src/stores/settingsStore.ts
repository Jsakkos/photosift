import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  nearDupThreshold: number;
  relatedThreshold: number;
  /// Maximum capture-time gap in seconds between two photos for them
  /// to cluster together. 0 disables the filter. Default 60s targets
  /// the "same burst" mental model.
  groupTimeWindowS: number;
  selectRequiresPick: boolean;
  routeMinStar: number;
  libraryRoot: string | null;
  enableAiOnImport: boolean;
  hideSoftThreshold: number;
  eyeOpenConfidence: number;
  /// Absolute path to the external ingest directory used by the
  /// Publish Direct export. Null when not configured; the export
  /// command returns a typed error so the UI can prompt first.
  immichIngestPath: string | null;
  /// AI Curator. When true, the import dialog's curator checkbox
  /// starts checked. API keys live in the OS keychain — see
  /// `set_curator_api_key(provider, ...)`.
  curatorDefaultRunOnImport: boolean;
  /// Legacy single-model field from before the multi-provider refactor.
  /// New code should read `curatorModelAnthropic` etc. instead.
  curatorModel: string;
  /// Hard ceiling on per-shoot curator spend, in cents. Worker stops
  /// dispatching new calls once exceeded. 0 = no cap (not recommended).
  /// Local provider always reports 0 cost so the cap is inert there.
  curatorMaxCostPerShootCents: number;
  /// Selected provider: "anthropic" | "gemini" | "local".
  curatorProvider: "anthropic" | "gemini" | "local";
  /// Per-provider model identifiers. The UI flips one of these into
  /// effect based on `curatorProvider` so users keep their last model
  /// choice when switching providers.
  curatorModelAnthropic: string;
  curatorModelGemini: string;
  curatorModelLocal: string;
  /// OpenAI-compatible base URL for the local provider, including the
  /// `/v1` suffix. Defaults to Ollama's port.
  curatorLocalBaseUrl: string;
}

const DEFAULT_SETTINGS: Settings = {
  nearDupThreshold: 4,
  relatedThreshold: 12,
  groupTimeWindowS: 60,
  selectRequiresPick: true,
  routeMinStar: 3,
  libraryRoot: null,
  enableAiOnImport: true,
  hideSoftThreshold: 30,
  eyeOpenConfidence: 0.7,
  immichIngestPath: null,
  curatorDefaultRunOnImport: true,
  curatorModel: "claude-sonnet-4-6",
  curatorMaxCostPerShootCents: 500,
  curatorProvider: "anthropic",
  curatorModelAnthropic: "claude-sonnet-4-6",
  curatorModelGemini: "gemini-2.5-flash",
  curatorModelLocal: "",
  curatorLocalBaseUrl: "http://localhost:11434/v1",
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
    const prev = get().settings;
    const next = { ...prev, ...partial };
    set({ settings: next });
    try {
      await invoke("update_settings", { settings: next });
    } catch (e) {
      // Roll back optimistic update on validation failure so the dialog can
      // surface the error and re-prompt.
      set({ settings: prev });
      console.error("Failed to persist settings:", e);
      throw e;
    }
  },

  reclusterShoot: async (shootId: number) => {
    return await invoke<number>("recluster_shoot", { shootId });
  },

  openDialog: () => set({ isOpen: true }),
  closeDialog: () => set({ isOpen: false }),
}));
