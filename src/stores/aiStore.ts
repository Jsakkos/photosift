import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  AiProgressEvent,
  AiProviderStatus,
  AiStatusResponse,
  EyeProviderKind,
  SharpnessPercentiles,
} from "../types";

interface AiState {
  provider: AiProviderStatus;
  eyeProvider: EyeProviderKind;
  analyzed: number;
  failed: number;
  total: number;
  percentiles: SharpnessPercentiles | null;
  setStatus: (s: AiStatusResponse) => void;
  setProvider: (p: AiProviderStatus) => void;
  handleProgress: (e: AiProgressEvent) => void;
  reset: () => void;
  fetchPercentiles: (shootId: number) => Promise<void>;
  setPercentiles: (p: SharpnessPercentiles | null) => void;
}

export const useAiStore = create<AiState>((set) => ({
  provider: "disabled",
  eyeProvider: "mock",
  analyzed: 0,
  failed: 0,
  total: 0,
  percentiles: null,
  setStatus: (s) =>
    set({
      provider: s.provider,
      eyeProvider: s.eyeProvider,
      analyzed: s.analyzed,
      failed: s.failed,
      total: s.total,
    }),
  setProvider: (p) => set({ provider: p }),
  handleProgress: (e) =>
    set({
      analyzed: Math.max(0, e.done - e.failed),
      failed: e.failed,
      total: e.total,
    }),
  reset: () => set({ analyzed: 0, failed: 0, total: 0, percentiles: null }),
  setPercentiles: (p) => set({ percentiles: p }),
  fetchPercentiles: async (shootId: number) => {
    try {
      const p = await invoke<SharpnessPercentiles>(
        "get_shoot_sharpness_percentiles",
        { shootId },
      );
      set({ percentiles: p });
    } catch (e) {
      console.error("fetchPercentiles failed:", e);
    }
  },
}));

/// Map a raw sharpness score (0-100) to a 1-10 badge value, using the
/// shoot's percentile cutoffs so the scale is relative to this shoot's
/// range rather than a fixed absolute threshold. When the shoot has no
/// analyzed photos yet, falls back to round(raw/10) so the badge still
/// renders something reasonable before percentiles arrive.
export function sharpnessBadgeScore(
  raw: number | null | undefined,
  p: SharpnessPercentiles | null,
): number {
  if (raw == null) return 1;
  if (!p || p.analyzedCount < 2) {
    const fallback = Math.round(raw / 10);
    return Math.max(1, Math.min(10, fallback));
  }
  if (raw >= p.p90) return 10;
  if (raw >= p.p70) return interp(raw, p.p70, p.p90, 8, 9);
  if (raw >= p.p50) return interp(raw, p.p50, p.p70, 6, 7);
  if (raw >= p.p30) return interp(raw, p.p30, p.p50, 4, 5);
  if (raw >= p.p10) return interp(raw, p.p10, p.p30, 2, 3);
  return 1;
}

function interp(v: number, lo: number, hi: number, outLo: number, outHi: number): number {
  if (hi <= lo) return outLo;
  const t = (v - lo) / (hi - lo);
  const s = outLo + t * (outHi - outLo);
  return Math.max(outLo, Math.min(outHi, Math.round(s)));
}

/// Green (8-10) / yellow (4-7) / red (1-3). Matches the Narrative Select
/// reference palette, mapped to this app's accent-aware dark theme.
export function sharpnessBandColor(score: number): "green" | "yellow" | "red" {
  if (score >= 8) return "green";
  if (score >= 4) return "yellow";
  return "red";
}
