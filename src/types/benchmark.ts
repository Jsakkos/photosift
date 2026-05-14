// TypeScript mirrors of the on-disk JSON shapes produced by the
// debug-only `benchmark_*` Tauri commands (see
// `src-tauri/src/commands/benchmark.rs`). Tagged with `camelCase` on
// the Rust side, so field names match directly.

/// Five mutually-exclusive subjective verdicts on a photo's sharpness.
/// `intended_bokeh` is the one we most care about — it represents the
/// f/1.8-style portraits where a low global sharpness score is correct
/// pixel-wise but wrong intent-wise. Correlating this tag against the
/// existing sharpness signals tells us which signal best tracks the
/// user's actual notion of "is this sharp where it matters".
export type SubjectSharpnessVerdict =
  | "subject_sharp"
  | "subject_blurry"
  | "all_sharp"
  | "all_blurry"
  | "intended_bokeh";

export const SUBJECT_SHARPNESS_VERDICTS: SubjectSharpnessVerdict[] = [
  "subject_sharp",
  "subject_blurry",
  "all_sharp",
  "all_blurry",
  "intended_bokeh",
];

export const SUBJECT_SHARPNESS_LABEL: Record<SubjectSharpnessVerdict, string> = {
  subject_sharp: "Subject sharp",
  subject_blurry: "Subject blurry",
  all_sharp: "All sharp",
  all_blurry: "All blurry",
  intended_bokeh: "Intended bokeh",
};

export interface SharpnessSnapshot {
  globalScore: number | null;
  maxEyeSharpness: number | null;
  meanEyeSharpness: number | null;
  /// The 1–10 bucket from AiSharpnessBadge at judgment time.
  aiSharpnessBadge1to10: number | null;
}

export interface BenchmarkFaceJudgment {
  /// Position in the `get_faces_for_photo` response. FaceRow has no
  /// primary key, so position is the only stable identifier across
  /// runs of the same analysis.
  faceIndex: number;
  /// Snapshot bbox at judgment time (normalized 0–1, [x, y, w, h]).
  bboxSnapshot: [number, number, number, number] | null;
  detectionCorrect: boolean | null;
  leftEyeCorrect: boolean | null;
  rightEyeCorrect: boolean | null;
  smileCorrect: boolean | null;
  speciesCorrect: boolean | null;
}

export interface BenchmarkPhotoRecord {
  photoId: number;
  shootId: number;
  cameraModel: string | null;
  judgedAt: string | null;
  faces: BenchmarkFaceJudgment[];
  /// Faces the user noticed that YuNet missed entirely.
  missedFaceCount: number;
  subjectSharpnessVerdict: SubjectSharpnessVerdict | null;
  sharpnessSignalsSnapshot: SharpnessSnapshot | null;
  notes: string;
}

export interface BenchmarkSetMeta {
  name: string;
  slug: string;
  createdAt: string;
  notes: string;
  schemaVersion: number;
}

export interface BenchmarkSet {
  set: BenchmarkSetMeta;
  photos: BenchmarkPhotoRecord[];
}

export interface BenchmarkSetListing {
  slug: string;
  name: string;
  createdAt: string;
  photoCount: number;
  judgedCount: number;
}

/// Constructs an empty face judgment with all fields null. Used when
/// the evaluator first opens a photo so the user can begin clicking.
export function emptyFaceJudgment(faceIndex: number): BenchmarkFaceJudgment {
  return {
    faceIndex,
    bboxSnapshot: null,
    detectionCorrect: null,
    leftEyeCorrect: null,
    rightEyeCorrect: null,
    smileCorrect: null,
    speciesCorrect: null,
  };
}

export function emptyPhotoRecord(
  photoId: number,
  shootId: number,
  cameraModel: string | null,
): BenchmarkPhotoRecord {
  return {
    photoId,
    shootId,
    cameraModel,
    judgedAt: null,
    faces: [],
    missedFaceCount: 0,
    subjectSharpnessVerdict: null,
    sharpnessSignalsSnapshot: null,
    notes: "",
  };
}
