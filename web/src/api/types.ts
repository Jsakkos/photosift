// Mirrors the server's JSON exactly (server/src/{events,ingest,db,api}). Field
// names stay snake_case so a response can be used without translation.

/** Naive camera-local time, `YYYY-MM-DDTHH:MM:SS` (no timezone is known). */
export type NaiveDateTime = string;

export type Outcome = "unreviewed" | "album" | "library" | "archive" | "delete";

export const OUTCOMES: readonly Outcome[] = [
  "unreviewed",
  "album",
  "library",
  "archive",
  "delete",
];

export type EventKind = "named" | "everyday";
export type ItemKind = "photo" | "video";
export type TimeSource = "exif" | "mtime";

// --- scan / import --------------------------------------------------------

export interface ScanItem {
  idx: number;
  primary: string;
  sibling_jpeg: string | null;
  kind: ItemKind;
  camera: string;
  capture_time: NaiveDateTime;
  time_source: TimeSource;
  bytes: number;
}

export interface CameraRange {
  camera: string;
  count: number;
  first: NaiveDateTime;
  last: NaiveDateTime;
}

export interface ProposedEvent {
  item_idxs: number[];
  start: NaiveDateTime;
  end: NaiveDateTime;
  kind: EventKind;
  suggested_name: string;
}

export interface CardScan {
  scan_id: string;
  source: string;
  items: ScanItem[];
  cameras: CameraRange[];
  already_imported: number;
  events: ProposedEvent[];
  warnings: string[];
}

/** One event to import; `name` is ignored for everyday events. */
export interface EventSpec {
  item_idxs: number[];
  kind: EventKind;
  name?: string;
}

export interface ImportRequest {
  scan_id: string;
  events: EventSpec[];
  /** Seconds to add to each camera's clock, keyed by camera string. */
  camera_offsets?: Record<string, number>;
}

export interface ImportStarted {
  job_id: string;
}

/** `GET /api/import/{job_id}`: where a job stands, for a page that missed events. */
export type JobStateReply =
  | { state: "running"; done: number; total: number }
  | {
      state: "done";
      shoot_ids: number[];
      copied: number;
      skipped_duplicates: number;
    }
  | { state: "failed"; error: string };

// --- shoots / review ------------------------------------------------------

export interface Counts {
  total: number;
  unreviewed: number;
  album: number;
  library: number;
  archive: number;
  delete: number;
}

export interface ShootSummary {
  id: number;
  name: string | null;
  kind: EventKind;
  folder: string;
  date_start: string | null;
  date_end: string | null;
  counts: Counts;
  enriched: boolean;
}

export interface Burst {
  idx: number;
  /** Best frame first. */
  photo_ids: number[];
}

export interface Moment {
  idx: number;
  start: string | null;
  end: string | null;
  bursts: Burst[];
}

export interface ReviewPhoto {
  id: number;
  w: number | null;
  h: number | null;
  capture_time: string | null;
  camera: string | null;
  outcome: Outcome;
  blooper: boolean;
  face_count: number;
  signal_score: number | null;
  has_video: boolean;
}

export interface ShootReview {
  shoot: ShootSummary;
  counts: Counts;
  highlights: number[];
  moments: Moment[];
  /** JSON object keys are strings even though they hold photo ids. */
  photos: Record<string, ReviewPhoto>;
}

// --- photos ---------------------------------------------------------------

export interface PhotoRow {
  id: number;
  shoot_id: number;
  raw_path: string | null;
  jpeg_path: string | null;
  video_path: string | null;
  sha256: string;
  camera: string | null;
  capture_time_orig: NaiveDateTime | null;
  capture_time: NaiveDateTime | null;
  time_source: TimeSource | null;
  orientation: number | null;
  width: number | null;
  height: number | null;
  /** A decimal string: a 64-bit hash does not fit a JS number. */
  phash: string | null;
  sharpness: number | null;
  face_sharpness: number | null;
  mean_luma: number | null;
  clip_hi: number | null;
  clip_lo: number | null;
  moment_idx: number | null;
  burst_idx: number | null;
  signal_score: number | null;
  outcome: Outcome;
  blooper: boolean;
  enriched_at: NaiveDateTime | null;
  trashed_at: NaiveDateTime | null;
}

export interface OutcomeResponse extends PhotoRow {
  /** Set when the sidecar could not be written; the outcome still stands. */
  xmp_error: string | null;
}

export interface TrashReport {
  trashed: number;
  failed: { photo_id: number; error: string }[];
}

/** The read-only settings the page needs. */
export interface ClientSettings {
  library_root: string;
}

// --- status / cards -------------------------------------------------------

export type AiBackend = "cuda" | "cpu" | "unavailable";

export interface ServerStatus {
  ai_backend: AiBackend;
  embeddings: boolean;
  queue: number;
}

export interface EventSummary {
  start: NaiveDateTime;
  end: NaiveDateTime;
  count: number;
  kind: EventKind;
}

/** What a card-detected toast needs; the full CardScan comes from /api/scan. */
export interface CardScanSummary {
  scan_id: string;
  source: string;
  photo_count: number;
  video_count: number;
  already_imported: number;
  cameras: CameraRange[];
  events: EventSummary[];
}

// --- live events ----------------------------------------------------------

export type ServerEvent =
  | { type: "card_detected"; scan: CardScanSummary }
  | { type: "card_removed"; source: string }
  | {
      type: "import_progress";
      job_id: string;
      done: number;
      total: number;
      current: string;
    }
  | {
      type: "import_done";
      job_id: string;
      shoot_ids: number[];
      copied: number;
      skipped_duplicates: number;
    }
  | { type: "import_failed"; job_id: string; error: string }
  | { type: "enrich_progress"; shoot_id: number; done: number; total: number }
  | { type: "enrich_done"; shoot_id: number };

export type ServerEventType = ServerEvent["type"];

export const SERVER_EVENT_TYPES: readonly ServerEventType[] = [
  "card_detected",
  "card_removed",
  "import_progress",
  "import_done",
  "import_failed",
  "enrich_progress",
  "enrich_done",
];
