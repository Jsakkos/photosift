export interface ImageEntry {
  id: number;
  filepath: string;
  filename: string;
  captureTime: string | null;
  cameraModel: string | null;
  lens: string | null;
  focalLength: number | null;
  aperture: number | null;
  shutterSpeed: string | null;
  iso: number | null;
  /// EXIF orientation (1–8). The preview and thumbnail on disk are
  /// already pre-rotated at ingest, so UI code only needs this for
  /// surfacing a "rotated" hint in the metadata overlay.
  orientation?: number | null;
  flag: string;
  destination: string;
  starRating: number;
  // AI fields (optional — populated by background worker)
  faceCount?: number | null;
  eyesOpenCount?: number | null;
  sharpnessScore?: number | null;
  /// Composite 0-100 quality score used for within-group ranking.
  /// Higher = better. Null when AI hasn't analyzed this photo yet.
  qualityScore?: number | null;
  aiAnalyzedAt?: string | null;
  /// Max smile confidence (0.0–1.0) across this photo's faces. Null when
  /// no mouth classifier is loaded or no faces were detected. Drives the
  /// smile factor in the AI pick formula.
  maxSmileScore?: number | null;
}

export interface ShootSummary {
  id: number;
  slug: string;
  date: string;
  sourcePath: string;
  destPath: string;
  photoCount: number;
  importedAt: string;
  // Cull-progress aggregates — backend computes these fresh on each
  // list_shoots call. Older backend responses may omit them; treat as
  // optional and default to 0 when rendering.
  picks?: number;
  rejects?: number;
  unreviewed?: number;
  // Most recent view_cursor row for this shoot, so the shoot card can
  // offer a "Continue [view]" CTA. Null/undefined when the user has
  // never opened the shoot.
  lastView?: CullView | null;
  lastOpenedAt?: string | null;
  /// Photo id chosen as the shoot's cover thumbnail; shows up on the
  /// shoot list. Null on shoots imported before this field existed.
  coverPhotoId?: number | null;
}

export type CullView = "triage" | "select" | "route";
export type ViewMode = "sequential" | "grid" | "comparison";

export interface DisplayItem {
  imageIndex: number;
  image: ImageEntry;
  groupId?: number;
  isGroupCover?: boolean;
  groupMemberCount?: number;
  isAiPick?: boolean;
}

export interface GroupMemberInfo {
  photoId: number;
  isCover: boolean;
}

export interface Group {
  id: number;
  shootId: number;
  groupType: "near_duplicate" | "related";
  members: GroupMemberInfo[];
}

export interface Face {
  photoId: number;
  bboxX: number; bboxY: number; bboxW: number; bboxH: number;
  leftEyeX: number; leftEyeY: number;
  rightEyeX: number; rightEyeY: number;
  leftEyeOpen: 0 | 1;
  rightEyeOpen: 0 | 1;
  leftEyeSharpness: number;
  rightEyeSharpness: number;
  detectionConfidence: number;
  /// Smile confidence 0.0–1.0 from the mouth classifier, or null when
  /// the mouth provider couldn't classify this face (model missing,
  /// crop out of bounds, etc.).
  smileScore: number | null;
  /// Subject species — `"human"` (from YuNet) or `"cat"` (from a future
  /// cat-face detector). The UI picks the icon variant from this.
  species: string;
}

export type AiProviderStatus = "cuda" | "cpu" | "disabled";

/// Which eye open/closed classifier the backend is running. `mock`
/// alternates deterministic 0/1 — not real signal, so the UI hides eye
/// indicators and ranks groups by sharpness alone until a real model ships.
export type EyeProviderKind = "mock" | "onnx";

/// Which mouth/smile classifier the backend is running. Mirrors
/// `EyeProviderKind`. UI gates smile icons + pick-formula smile factor
/// on `onnx` so the mock's fixed 0.5 output doesn't drive rankings.
export type MouthProviderKind = "mock" | "onnx";

export interface AiStatusResponse {
  provider: AiProviderStatus;
  eyeProvider: EyeProviderKind;
  mouthProvider: MouthProviderKind;
  analyzed: number;
  failed: number;
  total: number;
}

export interface AiProgressEvent {
  photoId: number;
  ok: boolean;
  done: number;
  total: number;
  failed: number;
}

/// Result of a Publish Direct export. Counts are mutually exclusive
/// (each candidate photo lands in exactly one bucket); `errors` is
/// bounded by the backend so an enormous shoot with many failures
/// doesn't stream thousands of messages to the UI.
export interface PublishDirectReport {
  copied: number;
  skipped: number;
  failed: number;
  destDir: string;
  errors: string[];
}

/// Sharpness percentile cutoffs for the current shoot. Mapped into the
/// 1-10 display badge in AiPanel so raw Laplacian scores remain
/// meaningful across shoots with different detail density.
export interface SharpnessPercentiles {
  p10: number;
  p30: number;
  p50: number;
  p70: number;
  p90: number;
  analyzedCount: number;
  analyzedMaxTs: string | null;
}
