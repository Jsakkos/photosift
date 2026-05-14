import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  BenchmarkFaceJudgment,
  BenchmarkPhotoRecord,
  BenchmarkSet,
  BenchmarkSetListing,
  SharpnessSnapshot,
  SubjectSharpnessVerdict,
} from "../types/benchmark";
import {
  emptyFaceJudgment,
  emptyPhotoRecord,
  SUBJECT_SHARPNESS_VERDICTS,
} from "../types/benchmark";

// ---- summary math (pure, exported for tests) ----------------------------

export interface BinaryAccuracy {
  correct: number;
  total: number;
}

export interface FaceDetectionStats {
  truePositive: number;
  falsePositive: number;
  missed: number;
  precision: number | null;
  recall: number | null;
  f1: number | null;
}

export interface SharpnessSignalAgg {
  mean: number | null;
  stdev: number | null;
  count: number;
}

export interface SharpnessVerdictGroup {
  verdict: SubjectSharpnessVerdict;
  count: number;
  globalScore: SharpnessSignalAgg;
  maxEyeSharpness: SharpnessSignalAgg;
  meanEyeSharpness: SharpnessSignalAgg;
  aiSharpnessBadge1to10: SharpnessSignalAgg;
}

export interface BenchmarkSummary {
  setName: string;
  totalPhotos: number;
  judgedPhotos: number;
  faceOverall: FaceDetectionStats;
  facePerCamera: { cameraModel: string; stats: FaceDetectionStats }[];
  /// YuNet's eye-landmark placement accuracy — measured separately from
  /// the eye-state classifier because a landmark on an eyebrow yields a
  /// classifier verdict of "closed" that is *correct given the input*,
  /// even though the underlying signal is wrong. Stratifying landmark
  /// vs. classifier errors is the whole reason this exists.
  landmark: BinaryAccuracy;
  leftEye: BinaryAccuracy;
  rightEye: BinaryAccuracy;
  /// Eye accuracy restricted to faces where the user marked the
  /// landmark correct. Lets us answer "given a good crop, how often
  /// does the classifier get it right?" — closer to a fair classifier
  /// score than the global numbers, which are landmark+classifier
  /// compounded.
  leftEyeGivenLandmarkOk: BinaryAccuracy;
  rightEyeGivenLandmarkOk: BinaryAccuracy;
  smile: BinaryAccuracy;
  species: BinaryAccuracy;
  sharpnessGroups: SharpnessVerdictGroup[];
}

function aggregateNumbers(values: number[]): SharpnessSignalAgg {
  if (values.length === 0) return { mean: null, stdev: null, count: 0 };
  const sum = values.reduce((a, b) => a + b, 0);
  const mean = sum / values.length;
  if (values.length === 1) return { mean, stdev: 0, count: 1 };
  const variance =
    values.reduce((acc, v) => acc + (v - mean) ** 2, 0) / values.length;
  return { mean, stdev: Math.sqrt(variance), count: values.length };
}

function divOrNull(numerator: number, denominator: number): number | null {
  if (denominator === 0) return null;
  return numerator / denominator;
}

function f1From(precision: number | null, recall: number | null): number | null {
  if (precision === null || recall === null) return null;
  if (precision + recall === 0) return 0;
  return (2 * precision * recall) / (precision + recall);
}

function judgmentIsEmpty(j: BenchmarkFaceJudgment): boolean {
  return (
    j.detectionCorrect === null &&
    j.landmarkCorrect === null &&
    j.leftEyeCorrect === null &&
    j.rightEyeCorrect === null &&
    j.smileCorrect === null &&
    j.speciesCorrect === null
  );
}

function photoIsJudged(p: BenchmarkPhotoRecord): boolean {
  if (p.judgedAt !== null) return true;
  if (p.subjectSharpnessVerdict !== null) return true;
  if (p.missedFaceCount > 0) return true;
  return p.faces.some((f) => !judgmentIsEmpty(f));
}

function faceStatsFor(photos: BenchmarkPhotoRecord[]): FaceDetectionStats {
  let tp = 0;
  let fp = 0;
  let missed = 0;
  for (const p of photos) {
    missed += p.missedFaceCount;
    for (const f of p.faces) {
      if (f.detectionCorrect === true) tp += 1;
      else if (f.detectionCorrect === false) fp += 1;
    }
  }
  const precision = divOrNull(tp, tp + fp);
  const recall = divOrNull(tp, tp + missed);
  return { truePositive: tp, falsePositive: fp, missed, precision, recall, f1: f1From(precision, recall) };
}

function binaryAccuracyFor(
  photos: BenchmarkPhotoRecord[],
  field: keyof Pick<
    BenchmarkFaceJudgment,
    | "landmarkCorrect"
    | "leftEyeCorrect"
    | "rightEyeCorrect"
    | "smileCorrect"
    | "speciesCorrect"
  >,
): BinaryAccuracy {
  let correct = 0;
  let total = 0;
  for (const p of photos) {
    for (const f of p.faces) {
      const v = f[field];
      if (v === null) continue;
      total += 1;
      if (v === true) correct += 1;
    }
  }
  return { correct, total };
}

/// Like `binaryAccuracyFor` but only counts faces where the user
/// confirmed the landmark was placed correctly. This isolates the
/// classifier's contribution from the landmark's contribution to any
/// eye-state error.
function eyeAccuracyGivenLandmarkOk(
  photos: BenchmarkPhotoRecord[],
  field: "leftEyeCorrect" | "rightEyeCorrect",
): BinaryAccuracy {
  let correct = 0;
  let total = 0;
  for (const p of photos) {
    for (const f of p.faces) {
      if (f.landmarkCorrect !== true) continue;
      const v = f[field];
      if (v === null) continue;
      total += 1;
      if (v === true) correct += 1;
    }
  }
  return { correct, total };
}

export function computeSummary(set: BenchmarkSet): BenchmarkSummary {
  const cameras = new Map<string, BenchmarkPhotoRecord[]>();
  for (const p of set.photos) {
    const key = p.cameraModel ?? "Unknown";
    const list = cameras.get(key) ?? [];
    list.push(p);
    cameras.set(key, list);
  }
  const facePerCamera = Array.from(cameras.entries())
    .map(([cameraModel, list]) => ({ cameraModel, stats: faceStatsFor(list) }))
    .sort((a, b) => a.cameraModel.localeCompare(b.cameraModel));

  const sharpnessGroups: SharpnessVerdictGroup[] = SUBJECT_SHARPNESS_VERDICTS.map(
    (verdict) => {
      const matching = set.photos.filter(
        (p) => p.subjectSharpnessVerdict === verdict,
      );
      const collect = (
        pick: (s: SharpnessSnapshot) => number | null | undefined,
      ): number[] =>
        matching
          .map((p) => p.sharpnessSignalsSnapshot)
          .filter((s): s is SharpnessSnapshot => s !== null)
          .map(pick)
          .filter((v): v is number => typeof v === "number" && Number.isFinite(v));

      return {
        verdict,
        count: matching.length,
        globalScore: aggregateNumbers(collect((s) => s.globalScore)),
        maxEyeSharpness: aggregateNumbers(collect((s) => s.maxEyeSharpness)),
        meanEyeSharpness: aggregateNumbers(collect((s) => s.meanEyeSharpness)),
        aiSharpnessBadge1to10: aggregateNumbers(
          collect((s) => s.aiSharpnessBadge1to10),
        ),
      };
    },
  );

  return {
    setName: set.set.name,
    totalPhotos: set.photos.length,
    judgedPhotos: set.photos.filter(photoIsJudged).length,
    faceOverall: faceStatsFor(set.photos),
    facePerCamera,
    landmark: binaryAccuracyFor(set.photos, "landmarkCorrect"),
    leftEye: binaryAccuracyFor(set.photos, "leftEyeCorrect"),
    rightEye: binaryAccuracyFor(set.photos, "rightEyeCorrect"),
    leftEyeGivenLandmarkOk: eyeAccuracyGivenLandmarkOk(set.photos, "leftEyeCorrect"),
    rightEyeGivenLandmarkOk: eyeAccuracyGivenLandmarkOk(
      set.photos,
      "rightEyeCorrect",
    ),
    smile: binaryAccuracyFor(set.photos, "smileCorrect"),
    species: binaryAccuracyFor(set.photos, "speciesCorrect"),
    sharpnessGroups,
  };
}

// ---- store --------------------------------------------------------------

type FaceVerdictField =
  | "detectionCorrect"
  | "landmarkCorrect"
  | "leftEyeCorrect"
  | "rightEyeCorrect"
  | "smileCorrect"
  | "speciesCorrect";

interface BenchmarkState {
  listings: BenchmarkSetListing[];
  isLoadingList: boolean;
  loadListError: string | null;

  currentSet: BenchmarkSet | null;
  currentPhotoIndex: number;
  currentFaceIndex: number;
  dirty: boolean;
  saveError: string | null;
  isSaving: boolean;

  refreshListings: () => Promise<void>;
  loadSet: (slug: string) => Promise<void>;
  createSet: (
    name: string,
    photos: BenchmarkPhotoRecord[],
  ) => Promise<BenchmarkSet | null>;
  saveSet: () => Promise<void>;
  deleteSet: (slug: string) => Promise<void>;
  closeSet: () => void;

  setPhotoIndex: (index: number) => void;
  setFaceIndex: (index: number) => void;

  /// Initialize an empty face judgment list when the evaluator first
  /// receives the face data for a photo. Idempotent — re-running
  /// preserves existing judgments while padding/truncating to match
  /// the new face count. Records the bbox and eye landmarks at this
  /// moment so the JSON pins each judgment to the exact AI output
  /// that was visible.
  ensureFaceJudgments: (
    faceCount: number,
    bboxes: [number, number, number, number][],
    leftEyes: [number, number][],
    rightEyes: [number, number][],
  ) => void;

  setFaceVerdict: (
    faceIndex: number,
    field: FaceVerdictField,
    value: boolean | null,
  ) => void;
  toggleFaceVerdict: (faceIndex: number, field: FaceVerdictField) => void;
  setMissedFaceCount: (count: number) => void;
  setSharpnessVerdict: (verdict: SubjectSharpnessVerdict | null) => void;
  setPhotoNotes: (notes: string) => void;
  setSharpnessSnapshot: (snapshot: SharpnessSnapshot) => void;
}

function nowIso(): string {
  return new Date().toISOString();
}

function withPhotoMutation(
  state: BenchmarkState,
  mutator: (photo: BenchmarkPhotoRecord) => BenchmarkPhotoRecord,
): Partial<BenchmarkState> {
  if (!state.currentSet) return {};
  const idx = state.currentPhotoIndex;
  const photo = state.currentSet.photos[idx];
  if (!photo) return {};
  const updated = mutator(photo);
  // Stamp `judgedAt` on first real edit.
  const judgedAt = updated.judgedAt ?? nowIso();
  const photos = [...state.currentSet.photos];
  photos[idx] = { ...updated, judgedAt };
  return {
    currentSet: { ...state.currentSet, photos },
    dirty: true,
  };
}

export const useBenchmarkStore = create<BenchmarkState>((set, get) => ({
  listings: [],
  isLoadingList: false,
  loadListError: null,

  currentSet: null,
  currentPhotoIndex: 0,
  currentFaceIndex: 0,
  dirty: false,
  saveError: null,
  isSaving: false,

  refreshListings: async () => {
    set({ isLoadingList: true, loadListError: null });
    try {
      const listings = await invoke<BenchmarkSetListing[]>("benchmark_list_sets");
      set({ listings, isLoadingList: false });
    } catch (e) {
      set({ isLoadingList: false, loadListError: String(e) });
    }
  },

  loadSet: async (slug: string) => {
    try {
      const benchmarkSet = await invoke<BenchmarkSet>("benchmark_load_set", { slug });
      set({
        currentSet: benchmarkSet,
        currentPhotoIndex: 0,
        currentFaceIndex: 0,
        dirty: false,
        saveError: null,
      });
    } catch (e) {
      set({ saveError: `Couldn't load set: ${e}` });
    }
  },

  createSet: async (name, photos) => {
    const trimmed = name.trim();
    const slug = slugify(trimmed || "untitled");
    const newSet: BenchmarkSet = {
      set: {
        name: trimmed || "Untitled",
        slug,
        createdAt: nowIso(),
        notes: "",
        schemaVersion: 1,
      },
      photos,
    };
    try {
      await invoke<BenchmarkSetListing>("benchmark_save_set", { set: newSet });
      set({
        currentSet: newSet,
        currentPhotoIndex: 0,
        currentFaceIndex: 0,
        dirty: false,
        saveError: null,
      });
      await get().refreshListings();
      return newSet;
    } catch (e) {
      set({ saveError: `Couldn't save set: ${e}` });
      return null;
    }
  },

  saveSet: async () => {
    const state = get();
    if (!state.currentSet) return;
    set({ isSaving: true, saveError: null });
    try {
      await invoke<BenchmarkSetListing>("benchmark_save_set", {
        set: state.currentSet,
      });
      set({ dirty: false, isSaving: false });
      await get().refreshListings();
    } catch (e) {
      set({ isSaving: false, saveError: `Couldn't save: ${e}` });
    }
  },

  deleteSet: async (slug: string) => {
    try {
      await invoke("benchmark_delete_set", { slug });
      const current = get().currentSet;
      if (current && current.set.slug === slug) {
        set({ currentSet: null, currentPhotoIndex: 0, currentFaceIndex: 0, dirty: false });
      }
      await get().refreshListings();
    } catch (e) {
      set({ saveError: `Couldn't delete: ${e}` });
    }
  },

  closeSet: () =>
    set({
      currentSet: null,
      currentPhotoIndex: 0,
      currentFaceIndex: 0,
      dirty: false,
      saveError: null,
    }),

  setPhotoIndex: (index: number) => {
    const state = get();
    if (!state.currentSet) return;
    const clamped = Math.max(0, Math.min(state.currentSet.photos.length - 1, index));
    set({ currentPhotoIndex: clamped, currentFaceIndex: 0 });
  },

  setFaceIndex: (index: number) => set({ currentFaceIndex: Math.max(0, index) }),

  ensureFaceJudgments: (faceCount, bboxes, leftEyes, rightEyes) => {
    set((state) =>
      withPhotoMutation(state, (photo) => {
        // Idempotent: keep existing judgments, pad with empties to match
        // the AI's current face count. The user can re-run AI later and
        // we'll just absorb any new faces here.
        const next: BenchmarkFaceJudgment[] = [];
        for (let i = 0; i < faceCount; i += 1) {
          const existing = photo.faces.find((f) => f.faceIndex === i);
          const bbox = bboxes[i] ?? null;
          const lEye = leftEyes[i] ?? null;
          const rEye = rightEyes[i] ?? null;
          if (existing) {
            // Keep the user's existing judgment; backfill any missing
            // snapshot from the current AI output without overwriting
            // a snapshot that was already captured.
            next.push({
              ...existing,
              bboxSnapshot: existing.bboxSnapshot ?? bbox,
              leftEyeSnapshot: existing.leftEyeSnapshot ?? lEye,
              rightEyeSnapshot: existing.rightEyeSnapshot ?? rEye,
            });
          } else {
            next.push({
              ...emptyFaceJudgment(i),
              bboxSnapshot: bbox,
              leftEyeSnapshot: lEye,
              rightEyeSnapshot: rEye,
            });
          }
        }
        return { ...photo, faces: next };
      }),
    );
  },

  setFaceVerdict: (faceIndex, field, value) => {
    set((state) =>
      withPhotoMutation(state, (photo) => {
        const faces = photo.faces.map((f) =>
          f.faceIndex === faceIndex ? { ...f, [field]: value } : f,
        );
        return { ...photo, faces };
      }),
    );
  },

  toggleFaceVerdict: (faceIndex, field) => {
    set((state) =>
      withPhotoMutation(state, (photo) => {
        const faces = photo.faces.map((f) => {
          if (f.faceIndex !== faceIndex) return f;
          const current = f[field];
          // Cycle: null → true → false → null. Three-state cycle on a
          // single key keeps the keyboard interaction symmetric for L/R
          // eye, smile, and species fields.
          const nextValue: boolean | null =
            current === null ? true : current === true ? false : null;
          return { ...f, [field]: nextValue };
        });
        return { ...photo, faces };
      }),
    );
  },

  setMissedFaceCount: (count) => {
    set((state) =>
      withPhotoMutation(state, (photo) => ({
        ...photo,
        missedFaceCount: Math.max(0, Math.floor(count)),
      })),
    );
  },

  setSharpnessVerdict: (verdict) => {
    set((state) =>
      withPhotoMutation(state, (photo) => ({
        ...photo,
        subjectSharpnessVerdict: verdict,
      })),
    );
  },

  setPhotoNotes: (notes) => {
    set((state) =>
      withPhotoMutation(state, (photo) => ({ ...photo, notes })),
    );
  },

  setSharpnessSnapshot: (snapshot) => {
    set((state) =>
      withPhotoMutation(state, (photo) => ({
        ...photo,
        // First-write wins — once captured, the snapshot is immutable.
        // The user's verdict refers to the AI signals as they were on
        // first view, not whatever the worker recomputes later.
        sharpnessSignalsSnapshot: photo.sharpnessSignalsSnapshot ?? snapshot,
      })),
    );
  },
}));

/// Exported for tests + the createSet helper. Mirrors the Rust slugifier
/// in `src-tauri/src/commands/benchmark.rs` — keep them in sync.
export function slugify(name: string): string {
  let out = "";
  let prevHyphen = true;
  for (const ch of name) {
    const isAlnum =
      (ch >= "0" && ch <= "9") ||
      (ch >= "a" && ch <= "z") ||
      (ch >= "A" && ch <= "Z");
    if (isAlnum) {
      out += ch.toLowerCase();
      prevHyphen = false;
    } else if (!prevHyphen) {
      out += "-";
      prevHyphen = true;
    }
  }
  while (out.endsWith("-")) out = out.slice(0, -1);
  return out.length > 0 ? out : "untitled";
}

// Exported `emptyPhotoRecord` re-export keeps callers from importing both
// the store module and the types module just to scaffold a new photo.
export { emptyPhotoRecord };
