import { create } from "zustand";
import type { AiProgressEvent, AiProviderStatus } from "../types";

interface AiState {
  provider: AiProviderStatus;
  analyzed: number;
  failed: number;
  total: number;
  setProvider: (p: AiProviderStatus) => void;
  handleProgress: (e: AiProgressEvent) => void;
  reset: () => void;
}

export const useAiStore = create<AiState>((set) => ({
  provider: "disabled",
  analyzed: 0,
  failed: 0,
  total: 0,
  setProvider: (p) => set({ provider: p }),
  handleProgress: (e) =>
    set({
      analyzed: Math.max(0, e.done - e.failed),
      failed: e.failed,
      total: e.total,
    }),
  reset: () => set({ analyzed: 0, failed: 0, total: 0 }),
}));
