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
  /// First time this photo was the focused frame in Select view. Null
  /// until the user lands on it. Read-only in the UI; the
  /// `mark_photo_visited_in_select` command stamps this server-side.
  selectVisitedAt?: string | null;
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
  /// Picks that already have a destination (edit or export). Lets the
  /// Library distinguish "triaged" (all flagged, routing pending) from
  /// "✓ routed" (every pick placed).
  routed?: number;
  /// Count of photos the user has focused at least once in Select. A
  /// non-zero value means the Select pass is in progress even if no
  /// stars have been assigned yet.
  selectVisited?: number;
  // Most recent view_cursor row for this shoot, so the shoot card can
  // offer a "Continue [view]" CTA. Null/undefined when the user has
  // never opened the shoot.
  lastView?: CullView | null;
  lastOpenedAt?: string | null;
  /// Photo id chosen as the shoot's cover thumbnail; shows up on the
  /// shoot list. Null on shoots imported before this field existed.
  coverPhotoId?: number | null;
  /// Modal camera body across the shoot's photos. Most-common non-null
  /// `photos.camera` (alphabetical tie-break); null when no photo carries
  /// a camera tag. Surfaced in the Library card metadata row.
  cameraModel?: string | null;
}

/// The four in-shoot views. `review` is a retrospective tab — it shows
/// tournament-bracket history rather than a cull queue — but it shares
/// the `currentView` / tab plumbing with the three cull passes.
export type CullView = "triage" | "select" | "route" | "review";
export type ViewMode = "sequential" | "grid";

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
  members: GroupMemberInfo[];
}

export interface Face {
  photoId: number;
  bboxX: number; bboxY: number; bboxW: number; bboxH: number;
  leftEyeX: number; leftEyeY: number;
  rightEyeX: number; rightEyeY: number;
  /// 0 = closed, 1 = open, null = no eye classifier was loaded at
  /// analysis time. Distinct from "all eyes closed". UI should treat
  /// null as "no data" rather than zero.
  leftEyeOpen: 0 | 1 | null;
  rightEyeOpen: 0 | 1 | null;
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

/// Which eye open/closed classifier the backend is running. `absent`
/// means no `eye_state.onnx` is loaded — the worker writes NULL for
/// `left_eye_open` / `right_eye_open`. The UI gates eye indicators on
/// `onnx` so the absent state shows up as "no data".
///
/// `mock` existed historically — it wrote alternating 0/1 noise that
/// looked real but wasn't. Removed; "absent" replaces it.
export type EyeProviderKind = "absent" | "onnx";

/// Which mouth/smile classifier the backend is running. Mirrors
/// `EyeProviderKind`. `absent` → smile_score is NULL on every face.
export type MouthProviderKind = "absent" | "onnx";

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

/// Summary of a `sync_shoot_layout` run. Returned from the
/// `sync_layout_if_eligible` IPC; `null` means the trigger was gated out
/// server-side and nothing ran.
export interface SyncReport {
  moved: { photoId: number; from: string; to: string }[];
  skippedAlreadyPlaced: number;
  missing: string[];
  collisions: string[];
  errors: string[];
}

/// A removable storage volume detected via `list_removable_drives`.
/// Used by the Import dialog's drive picker.
export interface DriveInfo {
  mountPoint: string;
  label: string | null;
  driveLetter: string | null;
  isRemovable: boolean;
  totalBytes: number;
  availableBytes: number;
}

/// One entry in the SD-card date browser scan. Cheap to produce — no
/// thumbnail decoding. `alreadyImported` is true when the heuristic dedup
/// matched an existing photo (camera + filename + size). `orientation` is
/// the EXIF tag (1–8) when present, used to render portrait tiles with
/// the correct aspect ratio.
export interface ScanDateEntry {
  path: string;
  filename: string;
  capturedAt: string | null;
  camera: string | null;
  fileSizeBytes: number;
  thumbDataUrl: string | null;
  alreadyImported: boolean;
  orientation?: number | null;
  /// When the camera was in RAW+JPEG mode and this tile represents a
  /// paired RAW+JPG, this is the JPG's absolute path. The tile shows
  /// a small "+JPG" badge to make the pair visible to the user; the
  /// JPG itself follows the RAW through layout moves at import time.
  siblingJpegPath?: string | null;
}

/// Lazy thumbnail extraction result, one per `scan-thumb-ready` event.
export interface ScanThumbReady {
  path: string;
  thumbDataUrl: string | null;
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

// ---- AI Curator (Claude) ----

export type CuratorSuggestedFlag = "pick" | "reject" | "keep";
export type CuratorUserAction = "accepted" | "overridden";
export type CuratorStatus = "idle" | "running" | "failed" | "disabled";
export type CuratorProvider = "anthropic" | "gemini" | "local";

/// One Claude judgment for one photo. Mirrors `curator_judgments` row
/// shape on the Rust side. `clusterRank` is null for singletons
/// (un-grouped photos analyzed in batches).
export interface CuratorJudgment {
  photoId: number;
  shootId: number;
  composition: number;        // 0-10
  aesthetic: number;          // 0-10
  clusterRank: number | null; // 1-based, null for singletons
  isKeeper: boolean;
  suggestedFlag: CuratorSuggestedFlag;
  reason: string;
  userAction: CuratorUserAction | null;
  judgedAt: string;
  /// Which provider produced this judgment: `"anthropic"`, `"gemini"`,
  /// or `"local"`. Surfaced as a small badge in `CuratorChip`.
  provider: string;
  model: string;
  promptVersion: number;
}

/// Stage 1 shoot characterization, stored in `shoots.curator_summary`.
export interface CuratorShootSummary {
  shoot_type: string;
  subjects: string[];
  story: string;
  dominant_style: string;
  watch_for: string[];
}

export interface CuratorRunStatus {
  status: CuratorStatus;
  runningShootId: number | null;
  processed: number;
  failed: number;
  total: number;
  costCents: number;
}

export interface CuratorAgreementStats {
  accepted: number;
  overridden: number;
  totalJudgments: number;
}

export interface AnthropicApiKeyStatus {
  configured: boolean;
  /// Last 4 chars of the key for display (e.g. `cdef`). Empty string
  /// when no key is configured.
  suffix: string;
}

/// Generic per-provider API key status. Returned by
/// `get_curator_api_key_status(provider)`. The Anthropic shape is kept
/// as `AnthropicApiKeyStatus` for backwards compatibility with existing
/// callers; new code should use this.
export interface ApiKeyStatus {
  configured: boolean;
  suffix: string;
}

/// Tauri events emitted from the curator worker. Frontend listens via
/// `@tauri-apps/api/event`. Names match the Rust `app.emit(...)` calls
/// in `curator/worker.rs`.
export interface CuratorProgressEvent {
  shootId: number;
  processed: number;
  total: number;
  costCents: number;
  stage?: string;
}
export interface CuratorClusterDoneEvent {
  shootId: number;
  groupId: number | null;
  processed: number;
  total: number;
  costCents: number;
}
export interface CuratorFailedEvent {
  shootId: number;
  groupId?: number | null;
  reason: string;
}
export interface CuratorCompletedEvent {
  shootId: number;
}

// ---- Curator triage stage (on-import first pass) ----

/// One triage-stage verdict for one photo. Mirrors a `triage_judgments`
/// row. The triage stage only ever emits `reject` or `keep`; `applied`
/// records whether the reject flag was actually written.
export interface TriageJudgment {
  photoId: number;
  shootId: number;
  suggestedFlag: "reject" | "keep";
  reason: string;
  applied: boolean;
  judgedAt: string;
  model: string;
  promptVersion: number;
}

/// Emitted once the triage stage finishes a shoot. `rejectPhotoIds` are
/// the photos the frontend should fold into a single batch undo entry.
export interface CuratorTriageDoneEvent {
  shootId: number;
  rejectPhotoIds: number[];
}

// ---- Tournament bracket history ----

export type BracketDecisionValue = "L" | "R" | "both" | "bye";
export type BracketDecisionSource = "user" | "curator";

/// One persisted tournament-bracket decision. Mirrors a
/// `bracket_decisions` row. `rightPhotoId` is null for a bye.
export interface BracketDecision {
  id: number;
  shootId: number;
  groupId: number;
  roundIndex: number;
  pairIndex: number;
  leftPhotoId: number;
  rightPhotoId: number | null;
  decision: BracketDecisionValue;
  decidedAt: string;
  source: BracketDecisionSource;
}
