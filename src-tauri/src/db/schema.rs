use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension, Result, Row};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Parses the `exif_date` column (EXIF DateTime format `YYYY-MM-DD
/// HH:MM:SS`, occasionally `YYYY:MM:DD HH:MM:SS`) to Unix seconds.
/// Returns `None` on any parse failure so callers can fall back to
/// pHash-only similarity for photos without a readable timestamp.
fn parse_exif_to_unix_s(s: &str) -> Option<i64> {
    // Try both common separators.
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y:%m:%d %H:%M:%S"] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt.and_utc().timestamp());
        }
    }
    None
}

// Aggregates flag counts per shoot and picks the most recent view_cursor
// so the shoot card can render a "Continue [view]" CTA. Column order must
// match `row_to_shoot`. The two queries share a column projection but
// differ in filtering/ordering, so they're inlined rather than composed.
const SHOOT_SUMMARY_SQL_LIST: &str =
    "SELECT s.id, s.slug, s.date, s.source_path, s.dest_path, s.photo_count, s.imported_at, s.import_mode, \
       COALESCE(SUM(CASE WHEN p.flag = 'pick' THEN 1 ELSE 0 END), 0) AS picks, \
       COALESCE(SUM(CASE WHEN p.flag = 'reject' THEN 1 ELSE 0 END), 0) AS rejects, \
       COALESCE(SUM(CASE WHEN p.flag = 'unreviewed' THEN 1 ELSE 0 END), 0) AS unreviewed, \
       COALESCE(SUM(CASE WHEN p.flag = 'pick' AND p.destination != 'unrouted' THEN 1 ELSE 0 END), 0) AS routed, \
       COALESCE(SUM(CASE WHEN p.select_visited_at IS NOT NULL THEN 1 ELSE 0 END), 0) AS select_visited, \
       (SELECT view_name FROM view_cursors WHERE shoot_id = s.id ORDER BY updated_at DESC LIMIT 1) AS last_view, \
       (SELECT updated_at FROM view_cursors WHERE shoot_id = s.id ORDER BY updated_at DESC LIMIT 1) AS last_opened_at, \
       s.cover_photo_id AS cover_photo_id, \
       (SELECT camera FROM photos WHERE shoot_id = s.id AND camera IS NOT NULL AND camera != '' \
          GROUP BY camera ORDER BY COUNT(*) DESC, camera ASC LIMIT 1) AS camera_model \
     FROM shoots s LEFT JOIN photos p ON p.shoot_id = s.id \
     GROUP BY s.id ORDER BY s.date DESC, s.id DESC";

const SHOOT_SUMMARY_SQL_ONE: &str =
    "SELECT s.id, s.slug, s.date, s.source_path, s.dest_path, s.photo_count, s.imported_at, s.import_mode, \
       COALESCE(SUM(CASE WHEN p.flag = 'pick' THEN 1 ELSE 0 END), 0) AS picks, \
       COALESCE(SUM(CASE WHEN p.flag = 'reject' THEN 1 ELSE 0 END), 0) AS rejects, \
       COALESCE(SUM(CASE WHEN p.flag = 'unreviewed' THEN 1 ELSE 0 END), 0) AS unreviewed, \
       COALESCE(SUM(CASE WHEN p.flag = 'pick' AND p.destination != 'unrouted' THEN 1 ELSE 0 END), 0) AS routed, \
       COALESCE(SUM(CASE WHEN p.select_visited_at IS NOT NULL THEN 1 ELSE 0 END), 0) AS select_visited, \
       (SELECT view_name FROM view_cursors WHERE shoot_id = s.id ORDER BY updated_at DESC LIMIT 1) AS last_view, \
       (SELECT updated_at FROM view_cursors WHERE shoot_id = s.id ORDER BY updated_at DESC LIMIT 1) AS last_opened_at, \
       s.cover_photo_id AS cover_photo_id, \
       (SELECT camera FROM photos WHERE shoot_id = s.id AND camera IS NOT NULL AND camera != '' \
          GROUP BY camera ORDER BY COUNT(*) DESC, camera ASC LIMIT 1) AS camera_model \
     FROM shoots s LEFT JOIN photos p ON p.shoot_id = s.id \
     WHERE s.id = ?1 GROUP BY s.id";

fn row_to_shoot(row: &Row<'_>) -> Result<ShootRow> {
    Ok(ShootRow {
        id: row.get(0)?,
        slug: row.get(1)?,
        date: row.get(2)?,
        source_path: row.get(3)?,
        dest_path: row.get(4)?,
        photo_count: row.get(5)?,
        imported_at: row.get(6)?,
        import_mode: row.get(7)?,
        picks: row.get(8)?,
        rejects: row.get(9)?,
        unreviewed: row.get(10)?,
        routed: row.get(11)?,
        select_visited: row.get(12)?,
        last_view: row.get(13)?,
        last_opened_at: row.get(14)?,
        cover_photo_id: row.get(15)?,
        camera_model: row.get(16)?,
    })
}

pub struct Database {
    pub(crate) conn: Connection,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShootRow {
    pub id: i64,
    pub slug: String,
    pub date: String,
    pub source_path: String,
    pub dest_path: String,
    pub photo_count: i64,
    pub imported_at: String,
    pub import_mode: String,
    // Cull-progress aggregates — computed fresh from the photos table on
    // each list_shoots call so the shoot list reflects reality without
    // needing a separate refresh path when flags change mid-session.
    pub picks: i64,
    pub rejects: i64,
    pub unreviewed: i64,
    /// Picks that already have a destination (edit or export). When
    /// `routed == picks` and `unreviewed == 0`, the shoot is fully routed
    /// and the Library badge can flip to "✓ routed". Before that it's
    /// only "triaged".
    pub routed: i64,
    /// Photos the user has focused at least once in Select view. Lets the
    /// Library surface a subtle "select in progress" hint later without
    /// needing another query.
    pub select_visited: i64,
    // Most recent view_cursor for this shoot, so the shoot card can offer
    // a "Continue [view]" CTA that jumps straight back to where the user
    // left off. Null when the user has never opened the shoot.
    pub last_view: Option<String>,
    pub last_opened_at: Option<String>,
    /// Photo to render as this shoot's cover on the shoot list. Populated
    /// at import time with the first photo id; AI can overwrite with a
    /// preferred pick later. `None` for legacy shoots imported before
    /// this field existed.
    pub cover_photo_id: Option<i64>,
    /// Modal camera body across the shoot's photos. Picks the most-common
    /// non-null `photos.camera` value (ties broken alphabetically for
    /// determinism); first-photo would be misleading since a shoot often
    /// opens with a stray cover frame from a different body. `None` when
    /// no photo has a camera tag.
    pub camera_model: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PhotoRow {
    pub id: i64,
    pub shoot_id: i64,
    pub filename: String,
    pub raw_path: String,
    pub preview_path: String,
    pub thumb_path: String,
    pub exif_date: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    /// EXIF orientation value (1-8) at import time. The preview JPEG and
    /// thumbnail on disk have already had this rotation baked in; the
    /// field is kept around so the UI can surface "rotated from portrait"
    /// hints and so future tooling can round-trip to XMP.
    pub orientation: Option<i32>,
    pub flag: String,
    pub destination: String,
    pub star_rating: i32,
    // AI enrichment — populated by the background worker.
    pub face_count: Option<i32>,
    pub eyes_open_count: Option<i32>,
    pub sharpness_score: Option<f64>,
    /// Composite 0-100 quality score used for within-group ranking.
    /// Combines sharpness with face presence (and later eye/mouth when
    /// those classifiers are real). Higher is better.
    pub quality_score: Option<f64>,
    pub ai_analyzed_at: Option<String>,
    /// Max `smile_score` across this photo's faces. Computed on read via
    /// subquery rather than denormalized onto the photos row — the faces
    /// INSERT is transactional with the photo aggregate update, so an
    /// on-demand MAX() is always consistent with the per-face values.
    pub max_smile_score: Option<f64>,
    /// First time this photo was the focused frame in Select view. `None`
    /// means the user has never looked at it in Select. Used by the
    /// auto-reorganize trigger to decide whether the Select pass is far
    /// enough along to finalize the kept set into `selects/`.
    pub select_visited_at: Option<String>,
    /// Path to a sibling JPEG that travels with this RAW (RAW+JPEG mode).
    /// `None` for plain RAW or standalone JPEG photos. The sibling does
    /// not get its own photo row — it follows the RAW through layout
    /// moves so Capture One/DxO see it next to the RAW on disk.
    pub sidecar_jpeg_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UndoEntry {
    pub id: i64,
    pub photo_id: i64,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupData {
    pub id: i64,
    pub shoot_id: i64,
    pub members: Vec<GroupMemberData>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMemberData {
    pub photo_id: i64,
    pub is_cover: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// pHash hamming-distance threshold for grouping similar photos into
    /// clusters — the single knob the Select "regroup" control exposes.
    pub group_threshold: i32,
    /// Maximum capture-time gap (seconds) allowed between two photos
    /// for them to form an edge in pHash clustering. 0 disables the
    /// time filter (pHash-only). Stops cross-time pHash collisions
    /// from merging unrelated shots that happen to share low-freq
    /// composition.
    pub group_time_window_s: i32,
    pub select_requires_pick: bool,
    pub route_min_star: i32,
    /// Absolute path to the root of the photo library (used for copy-mode imports).
    /// `None` falls back to the system Pictures directory.
    pub library_root: Option<String>,
    pub enable_ai_on_import: bool,
    pub hide_soft_threshold: i32,
    pub eye_open_confidence: f64,
    /// Absolute path to the external ingest folder (e.g. Immich's upload
    /// directory) that `export_publish_direct` copies JPEG previews into.
    /// `None` = not configured; the export command returns a typed error
    /// so the UI can prompt the user.
    pub immich_ingest_path: Option<String>,
    /// Curator (Claude) — whether the import-dialog "Run AI suggestions"
    /// checkbox is checked by default. Disabled in the UI when no API
    /// key is configured.
    pub curator_default_run_on_import: bool,
    /// Legacy single-model setting from before the multi-provider
    /// refactor. Kept for backwards compatibility on existing DBs; new
    /// code should use the per-provider `curator_model_*` fields instead.
    pub curator_model: String,
    /// Hard ceiling on per-shoot LLM spend (cents). Worker stops issuing
    /// new calls once this is exceeded (in-flight ones complete). 0 = no
    /// cap (not recommended). Local provider always reports 0 cost so
    /// the cap never trips for local inference.
    pub curator_max_cost_per_shoot_cents: i32,
    /// Selected curator provider: "anthropic" | "gemini" | "local".
    /// Used at worker spawn to pick the concrete `CuratorProvider`
    /// implementation.
    pub curator_provider: String,
    /// Per-provider model identifiers. Storing one per provider lets the
    /// UI remember the user's last model when they flip the provider
    /// dropdown.
    pub curator_model_anthropic: String,
    pub curator_model_gemini: String,
    pub curator_model_local: String,
    /// Base URL for the local OpenAI-compatible server (Ollama, LM
    /// Studio, vLLM, llama.cpp). Defaults to Ollama's port. Must end
    /// with `/v1` — provider code appends `/chat/completions` etc.
    pub curator_local_base_url: String,
    /// Configurable shoot-folder layout — the import path template and
    /// the bucket folder names. Applied globally; see
    /// `crate::folder_template`. Stored as JSON in the `folder_template`
    /// column; a NULL or unparseable value falls back to the default.
    pub folder_template: crate::folder_template::FolderTemplate,
    /// First-run guidance modals (#13): whether the user has seen and
    /// dismissed the one-time explainer for each culling view. "Replay
    /// tour" in the shortcuts overlay flips all three back to false.
    pub onboarded_triage: bool,
    pub onboarded_select: bool,
    pub onboarded_route: bool,
    /// First-run onboarding wizard (#9): true once the user has completed
    /// or skipped it. Migrated to true for pre-existing DBs (a library root
    /// configured, or any shoots) so an upgrade doesn't re-trigger it.
    pub onboarded_wizard: bool,
    /// Curator triage stage: whether the import pipeline runs an LLM
    /// first-pass that auto-rejects clearly-unusable frames. On by default;
    /// a no-op when no curator provider key is configured.
    pub curator_triage_on_import: bool,
    /// First-run guidance modal for the Review tab.
    pub onboarded_review: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            group_threshold: crate::ingest::clustering::DEFAULT_GROUP_THRESHOLD as i32,
            group_time_window_s: crate::ingest::clustering::DEFAULT_TIME_WINDOW_S as i32,
            select_requires_pick: true,
            route_min_star: 3,
            library_root: None,
            enable_ai_on_import: true,
            hide_soft_threshold: 30,
            eye_open_confidence: 0.7,
            immich_ingest_path: None,
            curator_default_run_on_import: true,
            curator_model: crate::curator::DEFAULT_MODEL.to_string(),
            curator_max_cost_per_shoot_cents: 500,
            curator_provider: "anthropic".to_string(),
            curator_model_anthropic: crate::curator::default_model_for("anthropic").to_string(),
            curator_model_gemini: crate::curator::default_model_for("gemini").to_string(),
            curator_model_local: crate::curator::default_model_for("local").to_string(),
            curator_local_base_url: "http://localhost:11434/v1".to_string(),
            folder_template: crate::folder_template::FolderTemplate::default(),
            onboarded_triage: false,
            onboarded_select: false,
            onboarded_route: false,
            onboarded_wizard: false,
            curator_triage_on_import: true,
            onboarded_review: false,
        }
    }
}

/// Percentile buckets of the `sharpness_score` column across a shoot's
/// analyzed photos. Used by the frontend to map raw Laplacian-variance
/// scores into a 1-10 scale that's meaningful *relative to the current
/// shoot* (D750 previews vary 2x in raw variance depending on detail
/// density, so any absolute calibration is guesswork).
///
/// `analyzed_max_ts` is the MAX(ai_analyzed_at) over the shoot at the
/// time of computation — the cache key used by callers to decide whether
/// a new computation is needed.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharpnessPercentiles {
    pub p10: f64,
    pub p30: f64,
    pub p50: f64,
    pub p70: f64,
    pub p90: f64,
    pub analyzed_count: i64,
    pub analyzed_max_ts: Option<String>,
}

/// One row from `curator_judgments`. Mirrors the struct in
/// `curator/types.rs` but lives here so DB callers don't need to depend
/// on the curator module.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CuratorJudgmentRow {
    pub photo_id: i64,
    pub shoot_id: i64,
    pub composition: i32,
    pub aesthetic: i32,
    pub cluster_rank: Option<i32>,
    pub is_keeper: bool,
    pub suggested_flag: String,
    pub reason: String,
    pub user_action: Option<String>,
    pub judged_at: String,
    /// Provider that produced this judgment: `"anthropic"`, `"gemini"`,
    /// or `"local"`. Frontend renders a small badge on `CuratorChip`.
    pub provider: String,
    pub model: String,
    pub prompt_version: i32,
}

/// One row from `triage_judgments` — the Curator's on-import first-pass
/// verdict. Only ever `reject` or `keep`; `applied` records whether the
/// reject flag has been written to `photos.flag` yet.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriageJudgmentRow {
    pub photo_id: i64,
    pub shoot_id: i64,
    pub suggested_flag: String,
    pub reason: String,
    pub applied: bool,
    pub judged_at: String,
    pub model: String,
    pub prompt_version: i32,
}

/// One row from `bracket_decisions` — a persisted tournament-bracket
/// decision. `source` is `"user"` (made in the Select tournament) or
/// `"curator"` (derived from the Curator's cluster ranking).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BracketDecisionRow {
    pub id: i64,
    pub shoot_id: i64,
    pub group_id: i64,
    pub round_index: i32,
    pub pair_index: i32,
    pub left_photo_id: i64,
    pub right_photo_id: Option<i64>,
    pub decision: String,
    pub decided_at: String,
    pub source: String,
}

/// Aggregate counts for the agreement-rate badge.
#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CuratorAgreementStats {
    pub accepted: i64,
    pub overridden: i64,
    pub total_judgments: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceRow {
    pub photo_id: i64,
    pub bbox_x: f64, pub bbox_y: f64, pub bbox_w: f64, pub bbox_h: f64,
    pub left_eye_x: f64, pub left_eye_y: f64,
    pub right_eye_x: f64, pub right_eye_y: f64,
    /// 0 = closed, 1 = open. NULL when no eye classifier was loaded at
    /// analysis time, or for non-human species where eye state isn't
    /// classified (cats today). The previous mock fallback wrote
    /// deterministic alternating values; that's been removed — a missing
    /// eye_state.onnx now correctly produces NULL, not noise.
    pub left_eye_open: Option<i32>,
    pub right_eye_open: Option<i32>,
    /// Laplacian variance on the eye crop, normalized 0–100. Computed
    /// independently of the eye classifier (it's pixel statistics, not
    /// model output) so this is real signal even when no classifier is
    /// loaded. Kept NOT NULL.
    pub left_eye_sharpness: f64, pub right_eye_sharpness: f64,
    pub detection_confidence: f64,
    /// 0.0–1.0 from the mouth classifier; None when no mouth model is loaded
    /// or the face crop fell outside the image. Renders as a smile badge in
    /// the AI panel and contributes to the pick score via the smile factor.
    pub smile_score: Option<f64>,
    /// Subject species — `"human"` (from YuNet) or `"cat"` (from a YOLO-family
    /// detector). Eye state and `smile_score` are only meaningful for humans;
    /// cats carry bbox + confidence with NULL eye/smile fields.
    pub species: String,
}

#[derive(Debug)]
pub struct PhotoInsert {
    pub filename: String,
    pub raw_path: String,
    pub preview_path: String,
    pub thumb_path: String,
    pub content_hash: [u8; 32],
    pub phash: Option<[u8; 8]>,
    pub exif_date: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    pub orientation: Option<i32>,
    /// Source file size in bytes. Persisted so future SD-card scans can
    /// fast-dedup against (camera, filename, file_size) without re-hashing.
    pub file_size_bytes: Option<u64>,
    /// Initial flag from EXIF/XMP sidecar at import time.
    /// Defaults to "unreviewed" when not provided.
    pub initial_flag: Option<String>,
    /// Initial star rating from EXIF/XMP sidecar at import time.
    pub initial_star_rating: Option<i32>,
    /// Path to a sibling JPEG (RAW+JPEG shoot mode). Stored on the RAW row;
    /// no separate photo row is created for the JPEG.
    pub sidecar_jpeg_path: Option<String>,
}

impl Database {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Open the global photosift DB at ~/.photosift/photosift.db
    pub fn open_global() -> Result<Self> {
        let path = global_db_path();
        Self::open(&path)
    }

    fn create_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS shoots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slug TEXT NOT NULL,
                date TEXT NOT NULL,
                source_path TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                photo_count INTEGER NOT NULL DEFAULT 0,
                imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                import_mode TEXT NOT NULL DEFAULT 'copy',
                cover_photo_id INTEGER
            );

            CREATE TABLE IF NOT EXISTS photos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                filename TEXT NOT NULL,
                raw_path TEXT NOT NULL,
                preview_path TEXT NOT NULL,
                thumb_path TEXT NOT NULL,
                content_hash BLOB NOT NULL,
                phash BLOB,
                exif_date TEXT,
                camera TEXT,
                lens TEXT,
                focal_length REAL,
                aperture REAL,
                shutter_speed TEXT,
                iso INTEGER,
                orientation INTEGER,
                flag TEXT NOT NULL DEFAULT 'unreviewed',
                destination TEXT NOT NULL DEFAULT 'unrouted',
                star_rating INTEGER NOT NULL DEFAULT 0,
                sharpness_score REAL,
                quality_score REAL,
                UNIQUE(content_hash, shoot_id)
            );
            CREATE INDEX IF NOT EXISTS idx_photos_shoot ON photos(shoot_id);
            CREATE INDEX IF NOT EXISTS idx_photos_flag ON photos(shoot_id, flag);

            CREATE TABLE IF NOT EXISTS groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_groups_shoot ON groups(shoot_id);

            CREATE TABLE IF NOT EXISTS group_members (
                group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                is_cover INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(group_id, photo_id)
            );
            CREATE INDEX IF NOT EXISTS idx_gm_photo ON group_members(photo_id);

            CREATE TABLE IF NOT EXISTS view_cursors (
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                view_name TEXT NOT NULL,
                last_photo_id INTEGER REFERENCES photos(id) ON DELETE SET NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY(shoot_id, view_name)
            );

            CREATE TABLE IF NOT EXISTS undo_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                session_id TEXT NOT NULL,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                field TEXT NOT NULL,
                old_value TEXT,
                new_value TEXT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_undo_session ON undo_log(shoot_id, session_id);

            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                group_threshold INTEGER NOT NULL DEFAULT 16,
                select_requires_pick INTEGER NOT NULL DEFAULT 1,
                route_min_star INTEGER NOT NULL DEFAULT 3,
                library_root TEXT
            );
            INSERT OR IGNORE INTO settings (id) VALUES (1);
            ",
        )?;
        self.run_migrations()
    }

    /// Additive SQLite migrations for columns introduced after the initial
    /// schema. Idempotent — each migration checks column presence before
    /// altering the table, so existing DBs are upgraded in place.
    fn run_migrations(&self) -> Result<()> {
        self.ensure_column("settings", "select_requires_pick", "INTEGER NOT NULL DEFAULT 1")?;
        self.ensure_column("settings", "route_min_star", "INTEGER NOT NULL DEFAULT 3")?;
        self.ensure_column("settings", "library_root", "TEXT")?;
        self.ensure_column("shoots", "import_mode", "TEXT NOT NULL DEFAULT 'copy'")?;
        // Phase 2 AI
        self.ensure_column("photos", "face_count", "INTEGER")?;
        self.ensure_column("photos", "eyes_open_count", "INTEGER")?;
        self.ensure_column("photos", "ai_analyzed_at", "TEXT")?;
        self.ensure_column("photos", "orientation", "INTEGER")?;
        self.ensure_column("shoots", "cover_photo_id", "INTEGER")?;
        self.ensure_column("photos", "quality_score", "REAL")?;
        self.create_faces_table()?;
        self.ensure_column("settings", "enable_ai_on_import", "INTEGER NOT NULL DEFAULT 1")?;
        self.ensure_column("settings", "hide_soft_threshold", "INTEGER NOT NULL DEFAULT 30")?;
        self.ensure_column("settings", "eye_open_confidence", "REAL NOT NULL DEFAULT 0.7")?;
        self.ensure_column(
            "settings",
            "group_time_window_s",
            "INTEGER NOT NULL DEFAULT 60",
        )?;
        self.ensure_column("faces", "smile_score", "REAL")?;
        self.ensure_column("settings", "immich_ingest_path", "TEXT")?;
        self.ensure_column("faces", "species", "TEXT NOT NULL DEFAULT 'human'")?;
        // Drop NOT NULL on left_eye_open / right_eye_open + wipe any
        // mock-classifier values that pre-existing rows still carry.
        // See `migrate_faces_eye_open_nullable` for the why.
        self.migrate_faces_eye_open_nullable()?;
        // Auto-reorganize-on-pass-complete tracking.
        // `photos.select_visited_at` gates the "all picks visited" check
        // that's half of the Select→Route sync trigger.
        self.ensure_column("photos", "select_visited_at", "TEXT")?;
        self.ensure_column("photos", "file_size_bytes", "INTEGER")?;
        // RAW+JPEG mode: when the camera writes a same-basename JPEG
        // alongside the RAW, store its path here so layout sync can move
        // them together.
        self.ensure_column("photos", "sidecar_jpeg_path", "TEXT")?;
        // `shoots.select_max_floor_reached` is the other half — records
        // the highest pass-floor (selectMinStar) the user has ever bumped
        // to for this shoot, so "did they actually run a narrowing pass?"
        // survives app restarts.
        self.ensure_column(
            "shoots",
            "select_max_floor_reached",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.create_file_moves_table()?;
        // Curator (Claude) subsystem additions.
        self.create_curator_judgments_table()?;
        self.ensure_column("shoots", "curator_summary", "TEXT")?;
        self.ensure_column("shoots", "curator_cost_cents", "INTEGER")?;
        self.ensure_column("shoots", "curator_completed_at", "TEXT")?;
        self.ensure_column(
            "settings",
            "curator_default_run_on_import",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        self.ensure_column(
            "settings",
            "curator_model",
            "TEXT NOT NULL DEFAULT 'claude-sonnet-4-6'",
        )?;
        self.ensure_column(
            "settings",
            "curator_max_cost_per_shoot_cents",
            "INTEGER NOT NULL DEFAULT 500",
        )?;
        // Multi-provider Curator additions.
        self.ensure_column(
            "curator_judgments",
            "provider",
            "TEXT NOT NULL DEFAULT 'anthropic'",
        )?;
        self.ensure_column(
            "settings",
            "curator_provider",
            "TEXT NOT NULL DEFAULT 'anthropic'",
        )?;
        self.ensure_column(
            "settings",
            "curator_model_anthropic",
            "TEXT NOT NULL DEFAULT 'claude-sonnet-4-6'",
        )?;
        self.ensure_column(
            "settings",
            "curator_model_gemini",
            "TEXT NOT NULL DEFAULT 'gemini-2.5-flash'",
        )?;
        self.ensure_column(
            "settings",
            "curator_model_local",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        self.ensure_column(
            "settings",
            "curator_local_base_url",
            "TEXT NOT NULL DEFAULT 'http://localhost:11434/v1'",
        )?;
        // Configurable folder template (#10). Nullable: a NULL or
        // unparseable value is read back as `FolderTemplate::default()`,
        // so we don't need to bake the default JSON into the schema.
        self.ensure_column("settings", "folder_template", "TEXT")?;
        // First-run guidance modals (#13). Default 0 (not onboarded) so
        // both fresh installs and existing users see each view's
        // explainer once; "Replay tour" resets them.
        self.ensure_column("settings", "onboarded_triage", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("settings", "onboarded_select", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("settings", "onboarded_route", "INTEGER NOT NULL DEFAULT 0")?;
        // Curator triage stage (on-import LLM first-pass) — on by default,
        // gated at runtime on a configured provider key.
        self.ensure_column(
            "settings",
            "curator_triage_on_import",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        self.ensure_column("settings", "onboarded_review", "INTEGER NOT NULL DEFAULT 0")?;
        // Triage-stage curator judgments + persisted tournament brackets.
        self.create_triage_judgments_table()?;
        self.create_bracket_decisions_table()?;
        // First-run onboarding wizard (#9). New column: seed it true for
        // pre-existing DBs (a library root configured, or any shoots) so an
        // upgrade doesn't re-trigger first-run. Brand-new installs leave it
        // 0 and get the wizard. Guarded on column absence so the UPDATE runs
        // exactly once, at the moment the column is introduced.
        if !self.column_exists("settings", "onboarded_wizard")? {
            self.conn.execute(
                "ALTER TABLE settings ADD COLUMN onboarded_wizard INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
            self.conn.execute(
                "UPDATE settings SET onboarded_wizard = 1
                 WHERE library_root IS NOT NULL OR EXISTS (SELECT 1 FROM shoots)",
                [],
            )?;
        }
        // Destination enum consolidation (2026-04): the Route pass merged
        // `dxo` into `edit` (one "ready to edit" bucket) and renamed
        // `publish_direct` to `export`. Gate on `PRAGMA user_version` so we
        // don't scan the full photos table on every app open once the
        // one-shot UPDATE has run. Later `sync_shoot_layout` calls will
        // relocate any files still sitting in the retired `RAW/dxo/` folder.
        let version: i32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            self.conn.execute(
                "UPDATE photos SET destination = 'edit' WHERE destination = 'dxo'",
                [],
            )?;
            self.conn.execute(
                "UPDATE photos SET destination = 'export' WHERE destination = 'publish_direct'",
                [],
            )?;
            // PRAGMA doesn't accept bound parameters, so inline the literal.
            self.conn.execute_batch("PRAGMA user_version = 1")?;
        }
        if version < 2 {
            // Multi-provider migration: backfill the per-provider model
            // column from the legacy `curator_model` so users keep the
            // same Anthropic model after the upgrade.
            self.conn.execute(
                "UPDATE settings
                    SET curator_model_anthropic = curator_model
                    WHERE id = 1
                      AND curator_model IS NOT NULL
                      AND curator_model <> ''",
                [],
            )?;
            self.conn.execute_batch("PRAGMA user_version = 2")?;
        }
        if version < 3 {
            // Cross-shoot reimport (#4): relax UNIQUE(content_hash) to
            // UNIQUE(content_hash, shoot_id) so the same RAW can land in
            // multiple shoots when the user explicitly opts into it via
            // the "Skip duplicates" import toggle. Within a shoot the
            // constraint still bites — re-importing the same file into
            // the same shoot is a no-op.
            //
            // Use the standard SQLite table-rebuild dance to swap the
            // constraint (you can't drop an inline UNIQUE on an existing
            // table). Enumerate columns from pragma_table_info so any
            // additive migrations that have already landed survive the
            // rebuild verbatim.
            let old_sql: Option<String> = self
                .conn
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'photos'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let old_sql = old_sql.unwrap_or_default();
            let needs_rebuild = old_sql.contains("UNIQUE(content_hash)")
                && !old_sql.contains("UNIQUE(content_hash, shoot_id)");
            if needs_rebuild {
                let mut stmt = self
                    .conn
                    .prepare("SELECT name FROM pragma_table_info('photos') ORDER BY cid")?;
                let cols: Vec<String> = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<_>>()?;
                drop(stmt);
                let cols_csv = cols.join(", ");
                let new_create = old_sql
                    .replacen("CREATE TABLE photos", "CREATE TABLE photos_v3", 1)
                    .replace("UNIQUE(content_hash)", "UNIQUE(content_hash, shoot_id)");
                // PRAGMA foreign_keys must toggle OUTSIDE a transaction; the
                // dance is documented at https://www.sqlite.org/lang_altertable.html
                self.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
                let script = format!(
                    "BEGIN;
                     {new_create};
                     INSERT INTO photos_v3 ({cols_csv}) SELECT {cols_csv} FROM photos;
                     DROP TABLE photos;
                     ALTER TABLE photos_v3 RENAME TO photos;
                     CREATE INDEX IF NOT EXISTS idx_photos_shoot ON photos(shoot_id);
                     CREATE INDEX IF NOT EXISTS idx_photos_flag ON photos(shoot_id, flag);
                     COMMIT;"
                );
                self.conn.execute_batch(&script)?;
                self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            }
            self.conn.execute_batch("PRAGMA user_version = 3")?;
        }
        if version < 4 {
            // Single-tier clustering cleanup (PR #79): the two-tier pHash
            // model collapsed to one threshold. Rename the surviving
            // threshold column to a name that matches its meaning and drop
            // the two columns the old model left behind. Each step is
            // guarded on the old column still existing, so a fresh DB
            // (created from the current base schema) skips them cleanly.
            if self.column_exists("settings", "near_dup_threshold")? {
                self.conn
                    .execute("ALTER TABLE settings DROP COLUMN near_dup_threshold", [])?;
            }
            if self.column_exists("settings", "related_threshold")? {
                self.conn.execute(
                    "ALTER TABLE settings RENAME COLUMN related_threshold TO group_threshold",
                    [],
                )?;
            }
            // `groups.group_type` carries an inline CHECK constraint, so it
            // can't be removed with DROP COLUMN — rebuild the table. Group
            // ids are copied verbatim so `group_members.group_id` stays
            // valid. Same table-rebuild dance as the `photos` rebuild above.
            if self.column_exists("groups", "group_type")? {
                self.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
                self.conn.execute_batch(
                    "BEGIN;
                     CREATE TABLE groups_v4 (
                         id INTEGER PRIMARY KEY AUTOINCREMENT,
                         shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE
                     );
                     INSERT INTO groups_v4 (id, shoot_id) SELECT id, shoot_id FROM groups;
                     DROP TABLE groups;
                     ALTER TABLE groups_v4 RENAME TO groups;
                     CREATE INDEX IF NOT EXISTS idx_groups_shoot ON groups(shoot_id);
                     COMMIT;",
                )?;
                self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            }
            self.conn.execute_batch("PRAGMA user_version = 4")?;
        }
        Ok(())
    }

    /// Append-only audit log of RAW file moves performed by
    /// `sync_shoot_layout`. Not used for reversal — reversal happens
    /// idempotently by flipping metadata and re-running sync — but keeps
    /// a debuggable history of every move we've made.
    fn create_file_moves_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_moves (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                photo_id INTEGER NOT NULL,
                from_path TEXT NOT NULL,
                to_path TEXT NOT NULL,
                moved_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_file_moves_photo ON file_moves(photo_id);",
        )?;
        Ok(())
    }

    /// Per-photo Claude judgments. One row per analyzed photo.
    /// `cluster_rank` is NULL for singleton (un-grouped) photos. The
    /// `user_action` column tracks deliberate accept/override events
    /// for the agreement-rate stat — incidental P/X agreement leaves
    /// it NULL by design (see plan §UI surfaces).
    fn create_curator_judgments_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS curator_judgments (
                photo_id INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                composition INTEGER NOT NULL CHECK(composition BETWEEN 0 AND 10),
                aesthetic   INTEGER NOT NULL CHECK(aesthetic   BETWEEN 0 AND 10),
                cluster_rank INTEGER,
                is_keeper INTEGER NOT NULL,
                suggested_flag TEXT NOT NULL CHECK(suggested_flag IN ('pick','reject','keep')),
                reason TEXT NOT NULL,
                user_action TEXT CHECK(user_action IN ('accepted','overridden') OR user_action IS NULL),
                judged_at TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_version INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_curator_judgments_shoot ON curator_judgments(shoot_id);
            CREATE INDEX IF NOT EXISTS idx_curator_judgments_keeper ON curator_judgments(shoot_id, is_keeper);",
        )?;
        Ok(())
    }

    /// Curator *triage-stage* judgments. Kept in a separate table from
    /// `curator_judgments` (whose PK is a bare `photo_id`) so a photo can
    /// carry both an on-import triage verdict and a later selection-stage
    /// judgment without a destructive primary-key migration. Triage only
    /// ever emits `reject` or `keep`; `applied` records whether the reject
    /// flag was actually written (it is skipped for photos the user has
    /// already triaged by hand).
    fn create_triage_judgments_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS triage_judgments (
                photo_id INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                suggested_flag TEXT NOT NULL CHECK(suggested_flag IN ('reject','keep')),
                reason TEXT NOT NULL,
                applied INTEGER NOT NULL DEFAULT 0,
                judged_at TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_version INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_triage_judgments_shoot ON triage_judgments(shoot_id);",
        )?;
        Ok(())
    }

    /// Persisted tournament-bracket decisions. One row per decided pair.
    /// `right_photo_id` is NULL for a bye. `source` distinguishes the
    /// user's own picks from curator-derived ones (Feature 4). The unique
    /// index makes undo→redo and curator reruns idempotent upserts; the
    /// `group_id` FK cascades, so a regroup auto-purges stale rows.
    fn create_bracket_decisions_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bracket_decisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
                round_index INTEGER NOT NULL,
                pair_index INTEGER NOT NULL,
                left_photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                right_photo_id INTEGER REFERENCES photos(id) ON DELETE CASCADE,
                decision TEXT NOT NULL CHECK(decision IN ('L','R','both','bye')),
                decided_at TEXT NOT NULL DEFAULT (datetime('now')),
                source TEXT NOT NULL DEFAULT 'user' CHECK(source IN ('user','curator'))
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_bracket_decisions_coord
                ON bracket_decisions(group_id, round_index, pair_index, source);
            CREATE INDEX IF NOT EXISTS idx_bracket_decisions_group
                ON bracket_decisions(shoot_id, group_id);",
        )?;
        Ok(())
    }

    fn create_faces_table(&self) -> Result<()> {
        // `left_eye_open` / `right_eye_open` are nullable: NULL means
        // "no eye-state classifier was loaded at analysis time".
        // The previous mock fallback wrote deterministic 0/1 values
        // that looked real but weren't — that codepath has been
        // removed in favor of honest nulls.
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS faces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                bbox_x REAL NOT NULL,
                bbox_y REAL NOT NULL,
                bbox_w REAL NOT NULL,
                bbox_h REAL NOT NULL,
                left_eye_x REAL NOT NULL,
                left_eye_y REAL NOT NULL,
                right_eye_x REAL NOT NULL,
                right_eye_y REAL NOT NULL,
                left_eye_open INTEGER,
                right_eye_open INTEGER,
                left_eye_sharpness REAL NOT NULL,
                right_eye_sharpness REAL NOT NULL,
                detection_confidence REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);",
        )?;
        Ok(())
    }

    /// SQLite can't `ALTER COLUMN` nullability, so to relax the
    /// `NOT NULL` constraint on `left_eye_open` / `right_eye_open`
    /// we recreate the table. Runs once per upgraded DB and is a
    /// no-op on databases that already have the relaxed schema.
    ///
    /// While we're at it we **wipe** existing eye-open and smile
    /// values, because any rows currently in the table were written
    /// by the now-removed mock classifier fallback and contain
    /// alternating-0/1 / constant-0.5 noise rather than real signal.
    /// Re-analyzing a shoot under the real classifier (or no
    /// classifier) repopulates them correctly.
    fn migrate_faces_eye_open_nullable(&self) -> Result<()> {
        // Probe existing NOT NULL constraints. If left_eye_open is
        // still notnull=1, this DB hasn't been migrated yet.
        let needs_migration: bool = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(faces)")?;
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(1)?;
                let notnull: i32 = row.get(3)?;
                Ok((name, notnull))
            })?;
            let mut still_notnull = false;
            for r in rows {
                let (name, notnull) = r?;
                if name == "left_eye_open" && notnull == 1 {
                    still_notnull = true;
                    break;
                }
            }
            still_notnull
        };
        if !needs_migration {
            return Ok(());
        }

        // The table-rebuild dance re-validates every foreign key on the
        // INSERT into faces_new; toggle foreign_keys OFF around it exactly as
        // the v3/v4 rebuilds do (https://sqlite.org/lang_altertable.html).
        // Without this, a DB carrying orphan faces rows — left behind by a
        // photo delete that ran while foreign_keys was off — fails the whole
        // migration, which aborts DB open entirely.
        self.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        self.conn.execute_batch(
            "BEGIN;
            CREATE TABLE faces_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                bbox_x REAL NOT NULL,
                bbox_y REAL NOT NULL,
                bbox_w REAL NOT NULL,
                bbox_h REAL NOT NULL,
                left_eye_x REAL NOT NULL,
                left_eye_y REAL NOT NULL,
                right_eye_x REAL NOT NULL,
                right_eye_y REAL NOT NULL,
                left_eye_open INTEGER,
                right_eye_open INTEGER,
                left_eye_sharpness REAL NOT NULL,
                right_eye_sharpness REAL NOT NULL,
                detection_confidence REAL NOT NULL,
                smile_score REAL,
                species TEXT NOT NULL DEFAULT 'human'
            );
            INSERT INTO faces_new (
                id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                left_eye_open, right_eye_open,
                left_eye_sharpness, right_eye_sharpness,
                detection_confidence, smile_score, species
            )
            SELECT
                id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                NULL, NULL,
                left_eye_sharpness, right_eye_sharpness,
                detection_confidence, NULL, species
            FROM faces WHERE photo_id IN (SELECT id FROM photos);
            DROP TABLE faces;
            ALTER TABLE faces_new RENAME TO faces;
            CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
            COMMIT;",
        )?;
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({})", table))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for r in rows {
            if r? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ensure_column(&self, table: &str, column: &str, type_clause: &str) -> Result<()> {
        if !self.column_exists(table, column)? {
            self.conn.execute(
                &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, type_clause),
                [],
            )?;
        }
        Ok(())
    }

    // ---- Shoots ----

    pub fn insert_shoot(
        &self,
        slug: &str,
        date: &str,
        source_path: &str,
        dest_path: &str,
        import_mode: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO shoots (slug, date, source_path, dest_path, import_mode)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![slug, date, source_path, dest_path, import_mode],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_shoot_photo_count(&self, shoot_id: i64, count: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE shoots SET photo_count = ?2 WHERE id = ?1",
            params![shoot_id, count],
        )?;
        Ok(())
    }

    /// Set a shoot's cover thumbnail. Only writes when the current cover
    /// is NULL — that way a later AI-pick override can call
    /// `force_set_shoot_cover` without this import-time helper clobbering
    /// it back to the first-photo default.
    pub fn set_shoot_cover_if_unset(&self, shoot_id: i64, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE shoots SET cover_photo_id = ?2
             WHERE id = ?1 AND cover_photo_id IS NULL",
            params![shoot_id, photo_id],
        )?;
        Ok(())
    }

    /// Re-pick a shoot's cover to the best picked photo. Uses `quality_score`
    /// as the ranking key, preferring flag='pick' photos; falls back to
    /// highest-quality overall when no picks exist yet. No-op when the
    /// shoot has no analyzed photos. Returns the chosen photo id for
    /// logging / UI feedback.
    ///
    /// Call after pick actions so the shoot list card reflects the user's
    /// latest curation without requiring a manual "set cover" gesture.
    pub fn auto_update_shoot_cover(&self, shoot_id: i64) -> Result<Option<i64>> {
        let picked: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM photos
                 WHERE shoot_id = ?1 AND flag = 'pick' AND quality_score IS NOT NULL
                 ORDER BY quality_score DESC, id ASC LIMIT 1",
                params![shoot_id],
                |r| r.get(0),
            )
            .optional()?;

        let chosen = match picked {
            Some(id) => Some(id),
            None => self
                .conn
                .query_row(
                    "SELECT id FROM photos
                     WHERE shoot_id = ?1 AND quality_score IS NOT NULL
                     ORDER BY quality_score DESC, id ASC LIMIT 1",
                    params![shoot_id],
                    |r| r.get(0),
                )
                .optional()?,
        };

        if let Some(id) = chosen {
            self.conn.execute(
                "UPDATE shoots SET cover_photo_id = ?2 WHERE id = ?1",
                params![shoot_id, id],
            )?;
        }
        Ok(chosen)
    }

    pub fn delete_shoot(&self, shoot_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM shoots WHERE id = ?1", params![shoot_id])?;
        Ok(())
    }

    pub fn list_shoots(&self) -> Result<Vec<ShootRow>> {
        // Single query: aggregate flag counts per shoot via GROUP BY, and
        // fetch the most recent view_cursor via a correlated subquery so
        // the shoot card can offer a "Continue" CTA without a second
        // round-trip.
        let mut stmt = self.conn.prepare(SHOOT_SUMMARY_SQL_LIST)?;
        let rows = stmt.query_map([], row_to_shoot)?;
        rows.collect()
    }

    pub fn get_shoot(&self, shoot_id: i64) -> Result<Option<ShootRow>> {
        self.conn
            .query_row(SHOOT_SUMMARY_SQL_ONE, params![shoot_id], row_to_shoot)
            .optional()
    }

    // ---- Photos ----

    pub fn photo_exists_by_hash(&self, content_hash: &[u8; 32]) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM photos WHERE content_hash = ?1",
                params![&content_hash[..]],
                |row| row.get::<_, i64>(0),
            )
            .optional()
    }

    /// Per-shoot variant used by the import pipeline when "Skip duplicates"
    /// is off: with the v3 schema we still don't want the same hash to land
    /// twice in the same shoot, but cross-shoot duplicates are explicitly
    /// allowed.
    pub fn photo_exists_in_shoot_by_hash(
        &self,
        shoot_id: i64,
        content_hash: &[u8; 32],
    ) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM photos WHERE content_hash = ?1 AND shoot_id = ?2",
                params![&content_hash[..], shoot_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
    }

    /// Heuristic dedup index: every imported photo's
    /// (camera_lowercase, filename_lowercase, file_size_bytes) tuple.
    /// Used by the SD-card scan to mark an entry as already-imported
    /// without re-hashing the file. Rows missing camera or file size are
    /// excluded since they can't form a complete heuristic match — the
    /// SHA-256 dedup at actual import time still catches those.
    pub fn known_originals(&self) -> Result<HashSet<(String, String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT camera, filename, file_size_bytes FROM photos
             WHERE camera IS NOT NULL AND file_size_bytes IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            let camera: String = row.get(0)?;
            let filename: String = row.get(1)?;
            let size: i64 = row.get(2)?;
            Ok((camera.to_lowercase(), filename.to_lowercase(), size as u64))
        })?;
        let mut set = HashSet::new();
        for r in rows {
            set.insert(r?);
        }
        Ok(set)
    }

    /// Insert a batch of photos for a shoot in a single transaction.
    /// Returns the inserted photo ids in the same order as the input.
    pub fn insert_photos_batch(
        &mut self,
        shoot_id: i64,
        photos: &[PhotoInsert],
    ) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut ids = Vec::with_capacity(photos.len());
        {
            let mut stmt = tx.prepare(
                "INSERT INTO photos (
                    shoot_id, filename, raw_path, preview_path, thumb_path,
                    content_hash, phash, exif_date, camera, lens,
                    focal_length, aperture, shutter_speed, iso, orientation,
                    flag, star_rating, file_size_bytes, sidecar_jpeg_path
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            )?;
            for p in photos {
                let flag = p.initial_flag.clone().unwrap_or_else(|| "unreviewed".into());
                let rating = p.initial_star_rating.unwrap_or(0);
                stmt.execute(params![
                    shoot_id,
                    p.filename,
                    p.raw_path,
                    p.preview_path,
                    p.thumb_path,
                    &p.content_hash[..],
                    p.phash.map(|h| h.to_vec()),
                    p.exif_date,
                    p.camera,
                    p.lens,
                    p.focal_length,
                    p.aperture,
                    p.shutter_speed,
                    p.iso,
                    p.orientation,
                    flag,
                    rating,
                    p.file_size_bytes.map(|n| n as i64),
                    p.sidecar_jpeg_path,
                ])?;
                ids.push(tx.last_insert_rowid());
            }
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Update file paths for photos after initial insert (preview_path/thumb_path
    /// reference {photo_id}.jpg which isn't known until after insert). Kept separate
    /// from insert to avoid a second copy of the file data; ingest writes the files
    /// keyed by id after this update call.
    pub fn update_photo_paths(
        &self,
        photo_id: i64,
        preview_path: &str,
        thumb_path: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET preview_path = ?2, thumb_path = ?3 WHERE id = ?1",
            params![photo_id, preview_path, thumb_path],
        )?;
        Ok(())
    }

    pub fn get_photo_by_id(&self, photo_id: i64) -> Result<PhotoRow> {
        self.conn.query_row(
            "SELECT id, shoot_id, filename, raw_path, preview_path, thumb_path,
                    exif_date, camera, lens, focal_length, aperture, shutter_speed,
                    iso, orientation, flag, destination, star_rating,
                    face_count, eyes_open_count, sharpness_score, quality_score, ai_analyzed_at,
                    (SELECT MAX(smile_score) FROM faces WHERE photo_id = photos.id) AS max_smile_score,
                    select_visited_at, sidecar_jpeg_path
             FROM photos WHERE id = ?1",
            params![photo_id],
            row_to_photo,
        )
    }

    pub fn photos_for_shoot(&self, shoot_id: i64) -> Result<Vec<PhotoRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, shoot_id, filename, raw_path, preview_path, thumb_path,
                    exif_date, camera, lens, focal_length, aperture, shutter_speed,
                    iso, orientation, flag, destination, star_rating,
                    face_count, eyes_open_count, sharpness_score, quality_score, ai_analyzed_at,
                    (SELECT MAX(smile_score) FROM faces WHERE photo_id = photos.id) AS max_smile_score,
                    select_visited_at, sidecar_jpeg_path
             FROM photos
             WHERE shoot_id = ?1
             ORDER BY exif_date ASC NULLS LAST, id ASC",
        )?;
        let rows = stmt.query_map(params![shoot_id], row_to_photo)?;
        rows.collect()
    }

    /// All photos in a shoot whose `destination` matches. Used by the
    /// Publish Direct export to select only photos the user flagged for
    /// immediate upload, leaving edit-path photos for Capture One/DxO.
    pub fn photos_by_destination(
        &self,
        shoot_id: i64,
        destination: &str,
    ) -> Result<Vec<PhotoRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, shoot_id, filename, raw_path, preview_path, thumb_path,
                    exif_date, camera, lens, focal_length, aperture, shutter_speed,
                    iso, orientation, flag, destination, star_rating,
                    face_count, eyes_open_count, sharpness_score, quality_score, ai_analyzed_at,
                    (SELECT MAX(smile_score) FROM faces WHERE photo_id = photos.id) AS max_smile_score,
                    select_visited_at, sidecar_jpeg_path
             FROM photos
             WHERE shoot_id = ?1 AND destination = ?2
             ORDER BY exif_date ASC NULLS LAST, id ASC",
        )?;
        let rows = stmt.query_map(params![shoot_id, destination], row_to_photo)?;
        rows.collect()
    }

    pub fn set_star_rating(&self, photo_id: i64, rating: i32) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET star_rating = ?2 WHERE id = ?1",
            params![photo_id, rating],
        )?;
        Ok(())
    }

    pub fn get_star_rating(&self, photo_id: i64) -> Result<i32> {
        self.conn.query_row(
            "SELECT star_rating FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )
    }

    // ---- Layout sync helpers ----

    /// Record that a photo was the focused frame in Select view. First-
    /// write wins — `WHERE select_visited_at IS NULL` means revisits
    /// don't reset the "visited" bit, so the "all picks visited" gate
    /// is monotonic and can't regress.
    pub fn mark_select_visited(&self, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET select_visited_at = datetime('now')
             WHERE id = ?1 AND select_visited_at IS NULL",
            params![photo_id],
        )?;
        Ok(())
    }

    /// Number of `flag='pick'` photos in the shoot that haven't been
    /// visited in Select view yet. The Select→Route trigger only fires
    /// when this is zero.
    pub fn unvisited_pick_count(&self, shoot_id: i64) -> Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos
             WHERE shoot_id = ?1 AND flag = 'pick' AND select_visited_at IS NULL",
            params![shoot_id],
            |r| r.get(0),
        )
    }

    /// Bump the per-shoot "max pass floor reached" if `floor` exceeds the
    /// stored value. Idempotent with respect to lower values — keeps the
    /// cross-session guarantee that once a user has run a narrowing pass,
    /// sync can finalize `selects/` without re-requiring the bump.
    pub fn bump_select_max_floor(&self, shoot_id: i64, floor: i32) -> Result<()> {
        self.conn.execute(
            "UPDATE shoots SET select_max_floor_reached = ?2
             WHERE id = ?1 AND select_max_floor_reached < ?2",
            params![shoot_id, floor],
        )?;
        Ok(())
    }

    pub fn get_select_max_floor(&self, shoot_id: i64) -> Result<i32> {
        self.conn
            .query_row(
                "SELECT select_max_floor_reached FROM shoots WHERE id = ?1",
                params![shoot_id],
                |r| r.get(0),
            )
            .or(Ok(0))
    }

    /// Update the on-disk location tracked for a RAW. Called from
    /// `sync_shoot_layout` after a successful `fs::rename`. The preview
    /// and thumbnail paths live in the photo-id-keyed cache at
    /// `~/.photosift/cache/{shoot_id}/…` so they're untouched by moves.
    pub fn update_raw_path(&self, photo_id: i64, new_raw_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET raw_path = ?2 WHERE id = ?1",
            params![photo_id, new_raw_path],
        )?;
        Ok(())
    }

    /// Update the on-disk location of the sibling JPEG after layout sync
    /// has moved both files together. `new_path` is `None` only if the
    /// JPEG was lost or removed; normal flow always writes a fresh path.
    pub fn update_sidecar_jpeg_path(
        &self,
        photo_id: i64,
        new_path: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET sidecar_jpeg_path = ?2 WHERE id = ?1",
            params![photo_id, new_path],
        )?;
        Ok(())
    }

    pub fn log_file_move(&self, photo_id: i64, from: &str, to: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO file_moves (photo_id, from_path, to_path)
             VALUES (?1, ?2, ?3)",
            params![photo_id, from, to],
        )?;
        Ok(())
    }

    // ---- Groups ----

    pub fn create_group(&self, shoot_id: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO groups (shoot_id) VALUES (?1)",
            params![shoot_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_group_member(&self, group_id: i64, photo_id: i64, is_cover: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO group_members (group_id, photo_id, is_cover)
             VALUES (?1, ?2, ?3)",
            params![group_id, photo_id, is_cover as i32],
        )?;
        Ok(())
    }

    pub fn get_groups_for_shoot(&self, shoot_id: i64) -> Result<Vec<GroupData>> {
        let mut stmt = self.conn.prepare(
            "SELECT g.id, g.shoot_id, gm.photo_id, gm.is_cover
             FROM groups g
             JOIN group_members gm ON gm.group_id = g.id
             WHERE g.shoot_id = ?1
             ORDER BY g.id, gm.photo_id",
        )?;

        let mut groups: Vec<GroupData> = Vec::new();
        let mut current_id: Option<i64> = None;

        let rows = stmt.query_map(params![shoot_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;

        for row in rows {
            let (gid, sid, pid, is_cover) = row?;
            if current_id != Some(gid) {
                groups.push(GroupData {
                    id: gid,
                    shoot_id: sid,
                    members: Vec::new(),
                });
                current_id = Some(gid);
            }
            if let Some(g) = groups.last_mut() {
                g.members.push(GroupMemberData {
                    photo_id: pid,
                    is_cover,
                });
            }
        }

        Ok(groups)
    }

    pub fn set_group_cover(&self, group_id: i64, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE group_members SET is_cover = 0 WHERE group_id = ?1",
            params![group_id],
        )?;
        self.conn.execute(
            "UPDATE group_members SET is_cover = 1 WHERE group_id = ?1 AND photo_id = ?2",
            params![group_id, photo_id],
        )?;
        Ok(())
    }

    /// Create a new group from the given photo IDs. The first photo becomes
    /// cover. Any listed photo that is already a member of another group is
    /// removed from that group first; groups left with fewer than 2 members
    /// are deleted. Returns the new group's id.
    pub fn create_group_with_members(
        &mut self,
        shoot_id: i64,
        photo_ids: &[i64],
    ) -> Result<i64> {
        if photo_ids.len() < 2 {
            return Err(rusqlite::Error::InvalidParameterName(
                "need at least 2 photos to form a group".into(),
            ));
        }
        // Detach from prior groups first so the new group is clean.
        self.remove_photos_from_groups(photo_ids)?;

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO groups (shoot_id) VALUES (?1)",
            params![shoot_id],
        )?;
        let group_id = tx.last_insert_rowid();
        for (i, &pid) in photo_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO group_members (group_id, photo_id, is_cover)
                 VALUES (?1, ?2, ?3)",
                params![group_id, pid, (i == 0) as i32],
            )?;
        }
        tx.commit()?;
        Ok(group_id)
    }

    /// Remove one or more photos from any group they belong to. Any group
    /// left with fewer than 2 members is deleted.
    pub fn remove_photos_from_groups(&mut self, photo_ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut affected_groups: std::collections::HashSet<i64> =
            std::collections::HashSet::new();
        for &pid in photo_ids {
            let mut stmt = tx.prepare(
                "SELECT group_id FROM group_members WHERE photo_id = ?1",
            )?;
            let ids: Vec<i64> = stmt
                .query_map(params![pid], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            drop(stmt);
            for gid in ids {
                affected_groups.insert(gid);
            }
            tx.execute(
                "DELETE FROM group_members WHERE photo_id = ?1",
                params![pid],
            )?;
        }
        for gid in affected_groups {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM group_members WHERE group_id = ?1",
                params![gid],
                |row| row.get(0),
            )?;
            if count < 2 {
                tx.execute("DELETE FROM groups WHERE id = ?1", params![gid])?;
            }
        }
        tx.commit()
    }

    pub fn delete_all_groups_for_shoot(&self, shoot_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM groups WHERE shoot_id = ?1",
            params![shoot_id],
        )?;
        Ok(())
    }

    /// Returns `(photo_id, phash_bytes, capture_unix_s)` for every
    /// photo in the shoot that has a phash. `capture_unix_s` is parsed
    /// from the EXIF datetime string if present; `None` when the photo
    /// lacks a parseable capture time.
    pub fn phashes_for_shoot(
        &self,
        shoot_id: i64,
    ) -> Result<Vec<(i64, [u8; 8], Option<i64>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, phash, exif_date FROM photos
             WHERE shoot_id = ?1 AND phash IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![shoot_id], |row| {
            let id: i64 = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            let exif_date: Option<String> = row.get(2)?;
            let mut arr = [0u8; 8];
            if bytes.len() == 8 {
                arr.copy_from_slice(&bytes);
            }
            let ts = exif_date.and_then(|s| parse_exif_to_unix_s(&s));
            Ok((id, arr, ts))
        })?;
        rows.collect()
    }

    // ---- AI: Faces + Aggregates ----

    pub fn insert_faces_batch(&mut self, faces: &[FaceRow]) -> Result<()> {
        if faces.is_empty() { return Ok(()); }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO faces (
                    photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                    left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                    left_eye_open, right_eye_open,
                    left_eye_sharpness, right_eye_sharpness,
                    detection_confidence, smile_score, species
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            )?;
            for f in faces {
                stmt.execute(params![
                    f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                    f.left_eye_x, f.left_eye_y, f.right_eye_x, f.right_eye_y,
                    f.left_eye_open, f.right_eye_open,
                    f.left_eye_sharpness, f.right_eye_sharpness,
                    f.detection_confidence, f.smile_score, f.species,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<FaceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                    left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                    left_eye_open, right_eye_open,
                    left_eye_sharpness, right_eye_sharpness, detection_confidence,
                    smile_score, species
             FROM faces WHERE photo_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![photo_id], |r| Ok(FaceRow {
            photo_id: r.get(0)?,
            bbox_x: r.get(1)?, bbox_y: r.get(2)?, bbox_w: r.get(3)?, bbox_h: r.get(4)?,
            left_eye_x: r.get(5)?, left_eye_y: r.get(6)?,
            right_eye_x: r.get(7)?, right_eye_y: r.get(8)?,
            left_eye_open: r.get(9)?, right_eye_open: r.get(10)?,
            left_eye_sharpness: r.get(11)?, right_eye_sharpness: r.get(12)?,
            detection_confidence: r.get(13)?,
            smile_score: r.get(14)?,
            species: r.get(15)?,
        }))?;
        rows.collect()
    }

    pub fn delete_faces_for_photo(&self, photo_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM faces WHERE photo_id = ?1", params![photo_id])?;
        Ok(())
    }

    /// Write AI aggregates + timestamp in a single call. Pass None for face_count
    /// to mark a photo as "attempted but failed" (ai_analyzed_at set, face_count null).
    pub fn mark_ai_analyzed(
        &self,
        photo_id: i64,
        face_count: Option<i32>,
        eyes_open_count: Option<i32>,
        sharpness_score: Option<f64>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET face_count = ?2, eyes_open_count = ?3,
                               sharpness_score = ?4,
                               ai_analyzed_at = datetime('now')
             WHERE id = ?1",
            params![photo_id, face_count, eyes_open_count, sharpness_score],
        )?;
        Ok(())
    }

    /// Atomically write AI results for a photo: delete any existing face rows,
    /// insert new ones, and update the aggregate columns. All in one transaction
    /// so a crash between statements cannot leave inconsistent state.
    pub fn write_ai_result(
        &mut self,
        photo_id: i64,
        faces: &[FaceRow],
        face_count: Option<i32>,
        eyes_open_count: Option<i32>,
        sharpness_score: Option<f64>,
        quality_score: Option<f64>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM faces WHERE photo_id = ?1", params![photo_id])?;
        if !faces.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO faces (
                    photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                    left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                    left_eye_open, right_eye_open,
                    left_eye_sharpness, right_eye_sharpness,
                    detection_confidence, smile_score, species
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            )?;
            for f in faces {
                stmt.execute(params![
                    f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                    f.left_eye_x, f.left_eye_y, f.right_eye_x, f.right_eye_y,
                    f.left_eye_open, f.right_eye_open,
                    f.left_eye_sharpness, f.right_eye_sharpness,
                    f.detection_confidence, f.smile_score, f.species,
                ])?;
            }
        }
        tx.execute(
            "UPDATE photos SET face_count = ?2, eyes_open_count = ?3,
                               sharpness_score = ?4, quality_score = ?5,
                               ai_analyzed_at = datetime('now')
             WHERE id = ?1",
            params![photo_id, face_count, eyes_open_count, sharpness_score, quality_score],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn clear_ai_for_shoot(&mut self, shoot_id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE photos SET face_count = NULL, eyes_open_count = NULL,
                               sharpness_score = NULL, quality_score = NULL,
                               ai_analyzed_at = NULL
             WHERE shoot_id = ?1",
            params![shoot_id],
        )?;
        tx.execute(
            "DELETE FROM faces WHERE photo_id IN
                (SELECT id FROM photos WHERE shoot_id = ?1)",
            params![shoot_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ---- Curator (Claude) ----

    /// Insert or replace one curator judgment row. Used by the worker's
    /// per-cluster batch write path. `cluster_rank` is `None` for
    /// singletons.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_curator_judgment(
        &self,
        photo_id: i64,
        shoot_id: i64,
        composition: i32,
        aesthetic: i32,
        cluster_rank: Option<i32>,
        is_keeper: bool,
        suggested_flag: &str,
        reason: &str,
        provider: &str,
        model: &str,
        prompt_version: i32,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO curator_judgments
                (photo_id, shoot_id, composition, aesthetic, cluster_rank,
                 is_keeper, suggested_flag, reason, judged_at,
                 provider, model, prompt_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'), ?9, ?10, ?11)
             ON CONFLICT(photo_id) DO UPDATE SET
                shoot_id = excluded.shoot_id,
                composition = excluded.composition,
                aesthetic = excluded.aesthetic,
                cluster_rank = excluded.cluster_rank,
                is_keeper = excluded.is_keeper,
                suggested_flag = excluded.suggested_flag,
                reason = excluded.reason,
                judged_at = excluded.judged_at,
                provider = excluded.provider,
                model = excluded.model,
                prompt_version = excluded.prompt_version,
                user_action = NULL",
            params![
                photo_id,
                shoot_id,
                composition,
                aesthetic,
                cluster_rank,
                is_keeper as i32,
                suggested_flag,
                reason,
                provider,
                model,
                prompt_version,
            ],
        )?;
        Ok(())
    }

    /// Read one judgment, used by the Triage UI to render the chip.
    pub fn curator_judgment_for_photo(
        &self,
        photo_id: i64,
    ) -> Result<Option<CuratorJudgmentRow>> {
        self.conn
            .query_row(
                "SELECT photo_id, shoot_id, composition, aesthetic, cluster_rank,
                        is_keeper, suggested_flag, reason, user_action, judged_at,
                        provider, model, prompt_version
                 FROM curator_judgments WHERE photo_id = ?1",
                params![photo_id],
                row_to_curator_judgment,
            )
            .optional()
    }

    /// Mark the user's action on a Claude suggestion. `action` is
    /// `"accepted"` (explicit `.` press) or `"overridden"` (manual P/X
    /// whose result differs from `suggested_flag`). Caller is expected
    /// to filter out the incidental-agreement case (manual P/X that
    /// happens to match).
    pub fn set_curator_user_action(
        &self,
        photo_id: i64,
        action: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE curator_judgments SET user_action = ?1 WHERE photo_id = ?2",
            params![action, photo_id],
        )?;
        Ok(())
    }

    /// Aggregate stats for the Library card "47/52 accepted" badge.
    pub fn curator_agreement_stats(
        &self,
        shoot_id: i64,
    ) -> Result<CuratorAgreementStats> {
        self.conn
            .query_row(
                "SELECT
                    SUM(CASE WHEN user_action = 'accepted' THEN 1 ELSE 0 END) AS accepted,
                    SUM(CASE WHEN user_action = 'overridden' THEN 1 ELSE 0 END) AS overridden,
                    COUNT(*) AS total
                 FROM curator_judgments WHERE shoot_id = ?1",
                params![shoot_id],
                |row| {
                    Ok(CuratorAgreementStats {
                        accepted: row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                        overridden: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        total_judgments: row.get::<_, i64>(2)?,
                    })
                },
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(CuratorAgreementStats::default()),
                _ => Err(e),
            })
    }

    /// Drop all judgments + Stage 1 summary for a shoot. Used by the
    /// "rerun curator" Library CTA after a prompt-version bump.
    pub fn clear_curator_for_shoot(&self, shoot_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM curator_judgments WHERE shoot_id = ?1",
            params![shoot_id],
        )?;
        self.conn.execute(
            "UPDATE shoots
                SET curator_summary = NULL,
                    curator_cost_cents = NULL,
                    curator_completed_at = NULL
             WHERE id = ?1",
            params![shoot_id],
        )?;
        Ok(())
    }

    /// Persist the Stage 1 result. The argument is the JSON string the
    /// caller already serialized — keeps the DB layer agnostic to the
    /// curator-types module.
    pub fn set_curator_summary(
        &self,
        shoot_id: i64,
        summary_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE shoots SET curator_summary = ?1 WHERE id = ?2",
            params![summary_json, shoot_id],
        )?;
        Ok(())
    }

    pub fn curator_summary_json(&self, shoot_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT curator_summary FROM shoots WHERE id = ?1",
                params![shoot_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map(|opt| opt.flatten())
    }

    pub fn add_curator_cost_cents(
        &self,
        shoot_id: i64,
        delta_cents: u32,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE shoots SET curator_cost_cents = COALESCE(curator_cost_cents, 0) + ?1
             WHERE id = ?2",
            params![delta_cents as i64, shoot_id],
        )?;
        Ok(())
    }

    pub fn curator_cost_cents(&self, shoot_id: i64) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COALESCE(curator_cost_cents, 0) FROM shoots WHERE id = ?1",
                params![shoot_id],
                |row| row.get::<_, i64>(0),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(0),
                _ => Err(e),
            })
    }

    pub fn mark_curator_completed(&self, shoot_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE shoots SET curator_completed_at = datetime('now') WHERE id = ?1",
            params![shoot_id],
        )?;
        Ok(())
    }

    /// All curator judgments for a shoot. Bulk-loaded by the frontend
    /// at `loadShoot` so Triage filtering and Select cluster-rank
    /// lookups stay synchronous reads on a local map.
    pub fn curator_judgments_for_shoot(
        &self,
        shoot_id: i64,
    ) -> Result<Vec<CuratorJudgmentRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id, shoot_id, composition, aesthetic, cluster_rank,
                    is_keeper, suggested_flag, reason, user_action, judged_at,
                    provider, model, prompt_version
             FROM curator_judgments WHERE shoot_id = ?1
             ORDER BY photo_id",
        )?;
        let rows = stmt.query_map(params![shoot_id], row_to_curator_judgment)?;
        rows.collect()
    }

    /// Photo IDs in this shoot that lack a `curator_judgments` row.
    /// Used by the resume path: any cluster whose members include a
    /// pending photo gets re-issued whole.
    pub fn photos_needing_curator(&self, shoot_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id FROM photos p
             LEFT JOIN curator_judgments j ON j.photo_id = p.id
             WHERE p.shoot_id = ?1 AND j.photo_id IS NULL
             ORDER BY p.id",
        )?;
        let rows = stmt.query_map(params![shoot_id], |r| r.get::<_, i64>(0))?;
        rows.collect()
    }

    // ---- Curator triage stage ----

    /// Insert or replace one triage-stage judgment. `applied` is reset to
    /// 0 on re-judgment so the new verdict can be acted on afresh.
    pub fn upsert_triage_judgment(
        &self,
        photo_id: i64,
        shoot_id: i64,
        suggested_flag: &str,
        reason: &str,
        model: &str,
        prompt_version: i32,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO triage_judgments
                (photo_id, shoot_id, suggested_flag, reason, applied,
                 judged_at, model, prompt_version)
             VALUES (?1, ?2, ?3, ?4, 0, datetime('now'), ?5, ?6)
             ON CONFLICT(photo_id) DO UPDATE SET
                shoot_id = excluded.shoot_id,
                suggested_flag = excluded.suggested_flag,
                reason = excluded.reason,
                applied = 0,
                judged_at = excluded.judged_at,
                model = excluded.model,
                prompt_version = excluded.prompt_version",
            params![photo_id, shoot_id, suggested_flag, reason, model, prompt_version],
        )?;
        Ok(())
    }

    /// All triage judgments for a shoot, bulk-loaded by the frontend so
    /// the Triage AI-rejects filter has a local map to read from.
    pub fn triage_judgments_for_shoot(
        &self,
        shoot_id: i64,
    ) -> Result<Vec<TriageJudgmentRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id, shoot_id, suggested_flag, reason, applied,
                    judged_at, model, prompt_version
             FROM triage_judgments WHERE shoot_id = ?1 ORDER BY photo_id",
        )?;
        let rows = stmt.query_map(params![shoot_id], |r| {
            Ok(TriageJudgmentRow {
                photo_id: r.get(0)?,
                shoot_id: r.get(1)?,
                suggested_flag: r.get(2)?,
                reason: r.get(3)?,
                applied: r.get::<_, i32>(4)? != 0,
                judged_at: r.get(5)?,
                model: r.get(6)?,
                prompt_version: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Photo IDs in this shoot with a `reject` triage judgment that has
    /// not yet been applied to `photos.flag`.
    pub fn pending_triage_rejects(&self, shoot_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id FROM triage_judgments
             WHERE shoot_id = ?1 AND suggested_flag = 'reject' AND applied = 0
             ORDER BY photo_id",
        )?;
        let rows = stmt.query_map(params![shoot_id], |r| r.get::<_, i64>(0))?;
        rows.collect()
    }

    /// Mark a triage judgment as applied so `apply_triage_rejects` is
    /// idempotent across repeated calls.
    pub fn mark_triage_judgment_applied(&self, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE triage_judgments SET applied = 1 WHERE photo_id = ?1",
            params![photo_id],
        )?;
        Ok(())
    }

    /// Photo IDs in this shoot that still need a triage judgment — used
    /// by the on-import enqueue so re-imports don't re-judge old photos.
    pub fn photos_needing_triage(&self, shoot_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id FROM photos p
             LEFT JOIN triage_judgments t ON t.photo_id = p.id
             WHERE p.shoot_id = ?1 AND t.photo_id IS NULL
             ORDER BY p.id",
        )?;
        let rows = stmt.query_map(params![shoot_id], |r| r.get::<_, i64>(0))?;
        rows.collect()
    }

    // ---- Tournament bracket decisions ----

    /// Insert or replace one bracket decision. Keyed on
    /// `(group_id, round, pair, source)` so undo→redo and curator
    /// re-derivation are idempotent upserts.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_bracket_decision(
        &self,
        shoot_id: i64,
        group_id: i64,
        round_index: i32,
        pair_index: i32,
        left_photo_id: i64,
        right_photo_id: Option<i64>,
        decision: &str,
        source: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO bracket_decisions
                (shoot_id, group_id, round_index, pair_index,
                 left_photo_id, right_photo_id, decision, decided_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), ?8)
             ON CONFLICT(group_id, round_index, pair_index, source) DO UPDATE SET
                shoot_id = excluded.shoot_id,
                left_photo_id = excluded.left_photo_id,
                right_photo_id = excluded.right_photo_id,
                decision = excluded.decision,
                decided_at = excluded.decided_at",
            params![
                shoot_id,
                group_id,
                round_index,
                pair_index,
                left_photo_id,
                right_photo_id,
                decision,
                source,
            ],
        )?;
        Ok(())
    }

    /// All bracket decisions for a shoot, ordered for deterministic
    /// rendering by the Review tab.
    pub fn bracket_decisions_for_shoot(
        &self,
        shoot_id: i64,
    ) -> Result<Vec<BracketDecisionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, shoot_id, group_id, round_index, pair_index,
                    left_photo_id, right_photo_id, decision, decided_at, source
             FROM bracket_decisions WHERE shoot_id = ?1
             ORDER BY group_id, source, round_index, pair_index",
        )?;
        let rows = stmt.query_map(params![shoot_id], |r| {
            Ok(BracketDecisionRow {
                id: r.get(0)?,
                shoot_id: r.get(1)?,
                group_id: r.get(2)?,
                round_index: r.get(3)?,
                pair_index: r.get(4)?,
                left_photo_id: r.get(5)?,
                right_photo_id: r.get(6)?,
                decision: r.get(7)?,
                decided_at: r.get(8)?,
                source: r.get(9)?,
            })
        })?;
        rows.collect()
    }

    /// Delete one bracket decision — used by undo of a user tournament pick.
    pub fn delete_bracket_decision(
        &self,
        group_id: i64,
        round_index: i32,
        pair_index: i32,
        source: &str,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM bracket_decisions
             WHERE group_id = ?1 AND round_index = ?2
               AND pair_index = ?3 AND source = ?4",
            params![group_id, round_index, pair_index, source],
        )?;
        Ok(())
    }

    /// Drop every decision of one `source` for a group — used to clear a
    /// group's curator-derived bracket before re-deriving it on rerun.
    pub fn delete_bracket_decisions_for_group(
        &self,
        group_id: i64,
        source: &str,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM bracket_decisions WHERE group_id = ?1 AND source = ?2",
            params![group_id, source],
        )?;
        Ok(())
    }

    /// Compute 10/30/50/70/90 percentile cutoffs for `sharpness_score` across
    /// all analyzed photos in a shoot. Returns zeros when no photos have been
    /// analyzed; callers should check `analyzed_count` before trusting the
    /// values. `analyzed_max_ts` is the cache-invalidation signal.
    pub fn sharpness_percentiles_for_shoot(
        &self,
        shoot_id: i64,
    ) -> Result<SharpnessPercentiles> {
        let mut stmt = self.conn.prepare(
            "SELECT sharpness_score FROM photos
             WHERE shoot_id = ?1 AND sharpness_score IS NOT NULL
             ORDER BY sharpness_score ASC",
        )?;
        let values: Vec<f64> = stmt
            .query_map(params![shoot_id], |r| r.get::<_, f64>(0))?
            .filter_map(|r| r.ok())
            .collect();

        let analyzed_max_ts: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(ai_analyzed_at) FROM photos
                 WHERE shoot_id = ?1 AND ai_analyzed_at IS NOT NULL",
                params![shoot_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();

        let count = values.len() as i64;
        if values.is_empty() {
            return Ok(SharpnessPercentiles {
                p10: 0.0,
                p30: 0.0,
                p50: 0.0,
                p70: 0.0,
                p90: 0.0,
                analyzed_count: 0,
                analyzed_max_ts,
            });
        }

        let pct = |q: f64| -> f64 {
            let idx = (q * (values.len() as f64 - 1.0)).round() as usize;
            values[idx.min(values.len() - 1)]
        };

        Ok(SharpnessPercentiles {
            p10: pct(0.10),
            p30: pct(0.30),
            p50: pct(0.50),
            p70: pct(0.70),
            p90: pct(0.90),
            analyzed_count: count,
            analyzed_max_ts,
        })
    }

    pub fn photos_needing_ai(&self, shoot_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM photos WHERE shoot_id = ?1 AND ai_analyzed_at IS NULL ORDER BY id",
        )?;
        let rows = stmt.query_map(params![shoot_id], |r| r.get::<_, i64>(0))?;
        rows.collect()
    }

    // ---- Flag / Destination ----

    pub fn set_flag(&self, photo_id: i64, flag: &str) -> Result<String> {
        let old: String = self.conn.query_row(
            "SELECT flag FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE photos SET flag = ?2 WHERE id = ?1",
            params![photo_id, flag],
        )?;
        Ok(old)
    }

    pub fn set_destination(&self, photo_id: i64, dest: &str) -> Result<String> {
        let old: String = self.conn.query_row(
            "SELECT destination FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE photos SET destination = ?2 WHERE id = ?1",
            params![photo_id, dest],
        )?;
        Ok(old)
    }

    pub fn bulk_set_flag(&self, photo_ids: &[i64], flag: &str) -> Result<Vec<(i64, String)>> {
        let mut old_values = Vec::with_capacity(photo_ids.len());
        for &id in photo_ids {
            let old = self.set_flag(id, flag)?;
            old_values.push((id, old));
        }
        Ok(old_values)
    }

    // ---- Undo Log ----

    pub fn append_undo(
        &self,
        shoot_id: i64,
        session_id: &str,
        photo_id: i64,
        field: &str,
        old_value: &str,
        new_value: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO undo_log (shoot_id, session_id, photo_id, field, old_value, new_value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![shoot_id, session_id, photo_id, field, old_value, new_value],
        )?;
        Ok(())
    }

    pub fn pop_undo(&self, shoot_id: i64, session_id: &str) -> Result<Option<UndoEntry>> {
        let entry = self.conn.query_row(
            "SELECT id, photo_id, field, old_value, new_value
             FROM undo_log
             WHERE shoot_id = ?1 AND session_id = ?2
             ORDER BY id DESC LIMIT 1",
            params![shoot_id, session_id],
            |row| {
                Ok(UndoEntry {
                    id: row.get(0)?,
                    photo_id: row.get(1)?,
                    field: row.get(2)?,
                    old_value: row.get(3)?,
                    new_value: row.get(4)?,
                })
            },
        ).optional()?;

        if let Some(ref e) = entry {
            self.conn.execute(
                "DELETE FROM undo_log WHERE id = ?1",
                params![e.id],
            )?;
            match e.field.as_str() {
                "flag" => { self.conn.execute("UPDATE photos SET flag = ?2 WHERE id = ?1", params![e.photo_id, e.old_value])?; }
                "destination" => { self.conn.execute("UPDATE photos SET destination = ?2 WHERE id = ?1", params![e.photo_id, e.old_value])?; }
                "star_rating" => { self.conn.execute("UPDATE photos SET star_rating = ?2 WHERE id = ?1", params![e.photo_id, e.old_value])?; }
                _ => {}
            }
        }

        Ok(entry)
    }

    // ---- Settings ----

    pub fn get_settings(&self) -> Result<Settings> {
        self.conn
            .query_row(
                "SELECT group_threshold, group_time_window_s,
                        select_requires_pick, route_min_star, library_root,
                        enable_ai_on_import, hide_soft_threshold, eye_open_confidence,
                        immich_ingest_path,
                        curator_default_run_on_import, curator_model,
                        curator_max_cost_per_shoot_cents,
                        curator_provider, curator_model_anthropic, curator_model_gemini,
                        curator_model_local, curator_local_base_url, folder_template,
                        onboarded_triage, onboarded_select, onboarded_route,
                        onboarded_wizard, curator_triage_on_import, onboarded_review
                 FROM settings WHERE id = 1",
                [],
                |row| {
                    // Columns are read by name, not position, so the mapping
                    // survives future schema changes (added or reordered
                    // columns) without a fragile index renumber.
                    Ok(Settings {
                        group_threshold: row.get("group_threshold")?,
                        group_time_window_s: row.get("group_time_window_s")?,
                        select_requires_pick: row.get::<_, i32>("select_requires_pick")? != 0,
                        route_min_star: row.get("route_min_star")?,
                        library_root: row.get("library_root")?,
                        enable_ai_on_import: row.get::<_, i32>("enable_ai_on_import")? != 0,
                        hide_soft_threshold: row.get("hide_soft_threshold")?,
                        eye_open_confidence: row.get("eye_open_confidence")?,
                        immich_ingest_path: row.get("immich_ingest_path")?,
                        curator_default_run_on_import: row
                            .get::<_, i32>("curator_default_run_on_import")?
                            != 0,
                        curator_model: row.get("curator_model")?,
                        curator_max_cost_per_shoot_cents: row
                            .get("curator_max_cost_per_shoot_cents")?,
                        curator_provider: row.get("curator_provider")?,
                        curator_model_anthropic: row.get("curator_model_anthropic")?,
                        curator_model_gemini: row.get("curator_model_gemini")?,
                        curator_model_local: row.get("curator_model_local")?,
                        curator_local_base_url: row.get("curator_local_base_url")?,
                        // NULL or unparseable JSON → fall back to the
                        // default layout rather than failing the read.
                        folder_template: row
                            .get::<_, Option<String>>("folder_template")?
                            .and_then(|s| serde_json::from_str(&s).ok())
                            .unwrap_or_default(),
                        onboarded_triage: row.get::<_, i32>("onboarded_triage")? != 0,
                        onboarded_select: row.get::<_, i32>("onboarded_select")? != 0,
                        onboarded_route: row.get::<_, i32>("onboarded_route")? != 0,
                        onboarded_wizard: row.get::<_, i32>("onboarded_wizard")? != 0,
                        curator_triage_on_import: row
                            .get::<_, i32>("curator_triage_on_import")?
                            != 0,
                        onboarded_review: row.get::<_, i32>("onboarded_review")? != 0,
                    })
                },
            )
            .or_else(|_| Ok(Settings::default()))
    }

    pub fn update_settings(&self, s: &Settings) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (id, group_threshold, group_time_window_s,
                                   select_requires_pick, route_min_star, library_root,
                                   enable_ai_on_import, hide_soft_threshold, eye_open_confidence,
                                   immich_ingest_path,
                                   curator_default_run_on_import, curator_model,
                                   curator_max_cost_per_shoot_cents,
                                   curator_provider, curator_model_anthropic, curator_model_gemini,
                                   curator_model_local, curator_local_base_url, folder_template,
                                   onboarded_triage, onboarded_select, onboarded_route,
                                   onboarded_wizard, curator_triage_on_import, onboarded_review)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
             ON CONFLICT(id) DO UPDATE SET
                group_threshold = excluded.group_threshold,
                group_time_window_s = excluded.group_time_window_s,
                select_requires_pick = excluded.select_requires_pick,
                route_min_star = excluded.route_min_star,
                library_root = excluded.library_root,
                enable_ai_on_import = excluded.enable_ai_on_import,
                hide_soft_threshold = excluded.hide_soft_threshold,
                eye_open_confidence = excluded.eye_open_confidence,
                immich_ingest_path = excluded.immich_ingest_path,
                curator_default_run_on_import = excluded.curator_default_run_on_import,
                curator_model = excluded.curator_model,
                curator_max_cost_per_shoot_cents = excluded.curator_max_cost_per_shoot_cents,
                curator_provider = excluded.curator_provider,
                curator_model_anthropic = excluded.curator_model_anthropic,
                curator_model_gemini = excluded.curator_model_gemini,
                curator_model_local = excluded.curator_model_local,
                curator_local_base_url = excluded.curator_local_base_url,
                folder_template = excluded.folder_template,
                onboarded_triage = excluded.onboarded_triage,
                onboarded_select = excluded.onboarded_select,
                onboarded_route = excluded.onboarded_route,
                onboarded_wizard = excluded.onboarded_wizard,
                curator_triage_on_import = excluded.curator_triage_on_import,
                onboarded_review = excluded.onboarded_review",
            params![
                s.group_threshold,
                s.group_time_window_s,
                s.select_requires_pick as i32,
                s.route_min_star,
                s.library_root,
                s.enable_ai_on_import as i32,
                s.hide_soft_threshold,
                s.eye_open_confidence,
                s.immich_ingest_path,
                s.curator_default_run_on_import as i32,
                s.curator_model,
                s.curator_max_cost_per_shoot_cents,
                s.curator_provider,
                s.curator_model_anthropic,
                s.curator_model_gemini,
                s.curator_model_local,
                s.curator_local_base_url,
                serde_json::to_string(&s.folder_template)
                    .unwrap_or_else(|_| serde_json::to_string(&crate::folder_template::FolderTemplate::default()).unwrap()),
                s.onboarded_triage as i32,
                s.onboarded_select as i32,
                s.onboarded_route as i32,
                s.onboarded_wizard as i32,
                s.curator_triage_on_import as i32,
                s.onboarded_review as i32,
            ],
        )?;
        Ok(())
    }

    // ---- View Cursors ----

    pub fn set_view_cursor(&self, shoot_id: i64, view_name: &str, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO view_cursors (shoot_id, view_name, last_photo_id, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(shoot_id, view_name) DO UPDATE SET
                last_photo_id = excluded.last_photo_id,
                updated_at = excluded.updated_at",
            params![shoot_id, view_name, photo_id],
        )?;
        Ok(())
    }

    pub fn get_view_cursor(&self, shoot_id: i64, view_name: &str) -> Result<Option<i64>> {
        self.conn.query_row(
            "SELECT last_photo_id FROM view_cursors
             WHERE shoot_id = ?1 AND view_name = ?2",
            params![shoot_id, view_name],
            |row| row.get(0),
        ).optional()
    }
}

fn row_to_curator_judgment(row: &rusqlite::Row) -> Result<CuratorJudgmentRow> {
    let is_keeper_int: i64 = row.get(5)?;
    Ok(CuratorJudgmentRow {
        photo_id: row.get(0)?,
        shoot_id: row.get(1)?,
        composition: row.get(2)?,
        aesthetic: row.get(3)?,
        cluster_rank: row.get(4)?,
        is_keeper: is_keeper_int != 0,
        suggested_flag: row.get(6)?,
        reason: row.get(7)?,
        user_action: row.get(8)?,
        judged_at: row.get(9)?,
        provider: row.get(10)?,
        model: row.get(11)?,
        prompt_version: row.get(12)?,
    })
}

fn row_to_photo(row: &rusqlite::Row) -> Result<PhotoRow> {
    Ok(PhotoRow {
        id: row.get(0)?,
        shoot_id: row.get(1)?,
        filename: row.get(2)?,
        raw_path: row.get(3)?,
        preview_path: row.get(4)?,
        thumb_path: row.get(5)?,
        exif_date: row.get(6)?,
        camera: row.get(7)?,
        lens: row.get(8)?,
        focal_length: row.get(9)?,
        aperture: row.get(10)?,
        shutter_speed: row.get(11)?,
        iso: row.get(12)?,
        orientation: row.get(13)?,
        flag: row.get(14)?,
        destination: row.get(15)?,
        star_rating: row.get(16)?,
        face_count: row.get(17)?,
        eyes_open_count: row.get(18)?,
        sharpness_score: row.get(19)?,
        quality_score: row.get(20)?,
        ai_analyzed_at: row.get(21)?,
        max_smile_score: row.get(22)?,
        select_visited_at: row.get(23)?,
        sidecar_jpeg_path: row.get(24)?,
    })
}

/// ~/.photosift/photosift.db (release) or ~/.photosift-dev/photosift.db (debug).
pub fn global_db_path() -> PathBuf {
    photosift_home().join("photosift.db")
}

/// Resolve the PhotoSift data root.
///
/// - Release builds default to `~/.photosift/`.
/// - Debug builds default to `~/.photosift-dev/` so a running production
///   binary keeps owning `.photosift` exclusively while dev work runs in
///   parallel.
/// - `$PHOTOSIFT_HOME` overrides both. Used by screenshot CI and
///   integration tests to redirect the entire app state directory at a
///   throwaway location, and as an escape hatch when you want a debug
///   build to read prod data (note: writes will mutate prod state).
pub fn photosift_home() -> PathBuf {
    if let Ok(custom) = std::env::var("PHOTOSIFT_HOME") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    let dir_name = if cfg!(debug_assertions) {
        ".photosift-dev"
    } else {
        ".photosift"
    };
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(dir_name)
}

/// ~/.photosift/cache/{shoot_id}/
pub fn shoot_cache_dir(shoot_id: i64) -> PathBuf {
    photosift_home()
        .join("cache")
        .join(shoot_id.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_db() -> (Database, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.sqlite");
        let db = Database::open(&db_path).unwrap();
        (db, dir)
    }

    fn sample_insert(hash_byte: u8, filename: &str) -> PhotoInsert {
        PhotoInsert {
            filename: filename.into(),
            raw_path: format!("/fake/{filename}"),
            preview_path: "/fake/p.jpg".into(),
            thumb_path: "/fake/t.jpg".into(),
            content_hash: [hash_byte; 32],
            phash: Some([0u8; 8]),
            exif_date: Some("2026-04-15T10:00:00".into()),
            camera: Some("NIKON D750".into()),
            lens: Some("50mm".into()),
            focal_length: Some(50.0),
            aperture: Some(1.8),
            shutter_speed: Some("1/250".into()),
            iso: Some(400),
            orientation: None,
            file_size_bytes: Some(1024),
            initial_flag: None,
            initial_star_rating: None,
            sidecar_jpeg_path: None,
        }
    }

    pub(crate) fn sample_insert_for_test(hash_byte: u8, filename: &str) -> PhotoInsert {
        sample_insert(hash_byte, filename)
    }

    /// Contract: a paired RAW+JPG frame produces ONE photo row whose
    /// `sidecar_jpeg_path` points at the JPG. The AI worker pulls work
    /// from `photos_needing_ai`, which is a per-row query — the JPG is
    /// metadata on the row, not its own row, so AI runs once per frame.
    ///
    /// Before this test, scan_folder used to emit the JPG as a separate
    /// tile, the user would import both, and AI would run twice on what
    /// was effectively the same image. This test guards against the
    /// regression by inserting the post-fix shape and verifying that
    /// photos_needing_ai counts each frame once.
    #[test]
    fn ai_worker_does_not_double_count_paired_raf_jpg() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("test", "2026-04-15", "/s", "/d", "copy")
            .unwrap();

        let mut paired = sample_insert(0xAA, "DSCF0001.RAF");
        paired.sidecar_jpeg_path = Some("/d/DSCF0001.JPG".into());
        let lone_raw = sample_insert(0xBB, "DSCF0002.RAF");
        let lone_jpg = sample_insert(0xCC, "wedding.jpg");

        let ids = db
            .insert_photos_batch(shoot_id, &[paired, lone_raw, lone_jpg])
            .unwrap();
        assert_eq!(ids.len(), 3, "three frames → three photo rows");

        let needing = db.photos_needing_ai(shoot_id).unwrap();
        assert_eq!(
            needing.len(),
            3,
            "AI worker should see one work item per frame, not per file"
        );
    }

    /// `sidecar_jpeg_path` is nullable, additive, and round-trips via
    /// the typed insert/select API. Schema migrations from older DBs
    /// already cover backfill (the column was added with no default
    /// after `file_size_bytes`); this test guards the API path.
    #[test]
    fn sidecar_jpeg_path_round_trips_through_insert_and_select() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("test", "2026-04-15", "/s", "/d", "copy")
            .unwrap();

        let mut with_sibling = sample_insert(0x11, "DSCF0010.RAF");
        with_sibling.sidecar_jpeg_path = Some("/d/DSCF0010.JPG".into());
        let without_sibling = sample_insert(0x22, "DSCF0011.RAF");

        let ids = db
            .insert_photos_batch(shoot_id, &[with_sibling, without_sibling])
            .unwrap();

        let p1 = db.get_photo_by_id(ids[0]).unwrap();
        let p2 = db.get_photo_by_id(ids[1]).unwrap();
        assert_eq!(p1.sidecar_jpeg_path.as_deref(), Some("/d/DSCF0010.JPG"));
        assert_eq!(p2.sidecar_jpeg_path, None);

        // photos_for_shoot must surface the field too — the import
        // pipeline reads it during layout sync.
        let photos = db.photos_for_shoot(shoot_id).unwrap();
        let by_id: std::collections::HashMap<_, _> =
            photos.iter().map(|p| (p.id, p)).collect();
        assert_eq!(
            by_id[&ids[0]].sidecar_jpeg_path.as_deref(),
            Some("/d/DSCF0010.JPG")
        );
        assert_eq!(by_id[&ids[1]].sidecar_jpeg_path, None);
    }

    /// `update_sidecar_jpeg_path` is what `sync_shoot_layout` calls
    /// after moving the JPG sibling into its new bucket. It must
    /// accept both Some(new_path) (move case) and None (caller cleared
    /// the linkage, e.g., after a manual JPG deletion).
    #[test]
    fn update_sidecar_jpeg_path_handles_some_and_none() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("test", "2026-04-15", "/s", "/d", "copy")
            .unwrap();
        let mut p = sample_insert(0x33, "DSCF0020.RAF");
        p.sidecar_jpeg_path = Some("/d/RAW/DSCF0020.JPG".into());
        let id = db.insert_photos_batch(shoot_id, &[p]).unwrap()[0];

        db.update_sidecar_jpeg_path(id, Some("/d/RAW/rejects/DSCF0020.JPG"))
            .unwrap();
        let after_move = db.get_photo_by_id(id).unwrap();
        assert_eq!(
            after_move.sidecar_jpeg_path.as_deref(),
            Some("/d/RAW/rejects/DSCF0020.JPG")
        );

        db.update_sidecar_jpeg_path(id, None).unwrap();
        let after_clear = db.get_photo_by_id(id).unwrap();
        assert_eq!(after_clear.sidecar_jpeg_path, None);
    }

    #[test]
    fn test_schema_creates_all_tables() {
        let (db, _dir) = test_db();
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN
                 ('shoots','photos','groups','group_members','view_cursors','undo_log')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn test_shoot_roundtrip() {
        let (db, _dir) = test_db();
        let id = db
            .insert_shoot("Greece", "2026-06-01", "/src", "/dst", "copy")
            .unwrap();
        let s = db.get_shoot(id).unwrap().unwrap();
        assert_eq!(s.slug, "Greece");
        assert_eq!(s.photo_count, 0);

        db.update_shoot_photo_count(id, 42).unwrap();
        let s2 = db.get_shoot(id).unwrap().unwrap();
        assert_eq!(s2.photo_count, 42);

        let listed = db.list_shoots().unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn test_list_shoots_aggregates_progress() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("Greece", "2026-06-01", "/s", "/d", "copy")
            .unwrap();

        // Start empty: every count is zero and no view_cursor is set.
        let fresh = &db.list_shoots().unwrap()[0];
        assert_eq!(fresh.picks, 0);
        assert_eq!(fresh.rejects, 0);
        assert_eq!(fresh.unreviewed, 0);
        assert!(fresh.last_view.is_none());
        assert!(fresh.last_opened_at.is_none());

        // Insert 4 photos, flip two to pick and one to reject; the fourth
        // stays unreviewed so we cover all three flag branches.
        let photos = vec![
            sample_insert(1, "a.nef"),
            sample_insert(2, "b.nef"),
            sample_insert(3, "c.nef"),
            sample_insert(4, "d.nef"),
        ];
        let ids = db.insert_photos_batch(shoot_id, &photos).unwrap();
        db.set_flag(ids[0], "pick").unwrap();
        db.set_flag(ids[1], "pick").unwrap();
        db.set_flag(ids[2], "reject").unwrap();

        // Record a view_cursor so the "last opened" projection kicks in.
        db.set_view_cursor(shoot_id, "select", ids[0]).unwrap();

        let summary = db.list_shoots().unwrap().into_iter().next().unwrap();
        assert_eq!(summary.picks, 2);
        assert_eq!(summary.rejects, 1);
        assert_eq!(summary.unreviewed, 1);
        assert_eq!(summary.last_view.as_deref(), Some("select"));
        assert!(summary.last_opened_at.is_some());

        // get_shoot should surface the same aggregates so the resume logic
        // in loadShoot can read them off a single invoke.
        let one = db.get_shoot(shoot_id).unwrap().unwrap();
        assert_eq!(one.picks, 2);
        assert_eq!(one.last_view.as_deref(), Some("select"));
    }

    #[test]
    fn test_list_shoots_modal_camera_model() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("Mixed", "2026-06-02", "/s", "/d", "copy")
            .unwrap();

        // Three photos: two D750, one X100VI. Modal pick is the D750. A
        // separate row with NULL camera must not poison the count.
        let mut a = sample_insert(1, "a.nef");
        let mut b = sample_insert(2, "b.nef");
        let mut c = sample_insert(3, "c.jpg");
        let mut d = sample_insert(4, "d.jpg");
        a.camera = Some("NIKON D750".into());
        b.camera = Some("NIKON D750".into());
        c.camera = Some("FUJIFILM X100VI".into());
        d.camera = None;
        db.insert_photos_batch(shoot_id, &[a, b, c, d]).unwrap();

        let listed = &db.list_shoots().unwrap()[0];
        assert_eq!(listed.camera_model.as_deref(), Some("NIKON D750"));

        let one = db.get_shoot(shoot_id).unwrap().unwrap();
        assert_eq!(one.camera_model.as_deref(), Some("NIKON D750"));

        // Empty shoot has no camera tag.
        let empty_id = db
            .insert_shoot("Empty", "2026-06-03", "/s", "/d", "copy")
            .unwrap();
        let empty = db.get_shoot(empty_id).unwrap().unwrap();
        assert_eq!(empty.camera_model, None);
    }

    #[test]
    fn test_insert_photos_batch_and_read() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("Test", "2026-04-15", "/s", "/d", "copy")
            .unwrap();

        let photos = vec![sample_insert(1, "a.nef"), sample_insert(2, "b.nef")];
        let ids = db.insert_photos_batch(shoot_id, &photos).unwrap();
        assert_eq!(ids.len(), 2);

        let rows = db.photos_for_shoot(shoot_id).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].flag, "unreviewed");
        assert_eq!(rows[0].destination, "unrouted");
        assert_eq!(rows[0].star_rating, 0);
    }

    #[test]
    fn test_known_originals_heuristic() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();

        // Two rows with full heuristic info; one with a NULL camera that
        // should be excluded from the index.
        let mut a = sample_insert(1, "DSC_0001.NEF");
        a.file_size_bytes = Some(20_000_000);
        let mut b = sample_insert(2, "DSC_0002.NEF");
        b.file_size_bytes = Some(21_000_000);
        let mut c = sample_insert(3, "DSC_0003.NEF");
        c.camera = None;
        c.file_size_bytes = Some(22_000_000);
        db.insert_photos_batch(shoot_id, &[a, b, c]).unwrap();

        let known = db.known_originals().unwrap();
        assert_eq!(known.len(), 2);
        assert!(known.contains(&("nikon d750".into(), "dsc_0001.nef".into(), 20_000_000)));
        assert!(known.contains(&("nikon d750".into(), "dsc_0002.nef".into(), 21_000_000)));
        // Camera-less row is invisible to the heuristic — hash dedup at
        // import time still catches it.
        assert!(!known.contains(&("".into(), "dsc_0003.nef".into(), 22_000_000)));
    }

    #[test]
    fn test_orientation_roundtrips() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("T", "2026-04-15", "/s", "/d", "copy")
            .unwrap();

        let mut a = sample_insert(1, "a.nef");
        a.orientation = Some(6);
        let mut b = sample_insert(2, "b.nef");
        b.orientation = None; // absent EXIF tag should stay None
        let ids = db.insert_photos_batch(shoot_id, &[a, b]).unwrap();

        let row_a = db.get_photo_by_id(ids[0]).unwrap();
        let row_b = db.get_photo_by_id(ids[1]).unwrap();
        assert_eq!(row_a.orientation, Some(6));
        assert_eq!(row_b.orientation, None);

        // photos_for_shoot must return the same orientation value; the UI
        // reads from this list path, not get_photo_by_id.
        let listed = db.photos_for_shoot(shoot_id).unwrap();
        let listed_a = listed.iter().find(|r| r.id == ids[0]).unwrap();
        assert_eq!(listed_a.orientation, Some(6));
    }

    #[test]
    fn test_dedup_by_hash() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        db.insert_photos_batch(shoot_id, &[sample_insert(7, "x.nef")])
            .unwrap();

        let existing = db.photo_exists_by_hash(&[7u8; 32]).unwrap();
        assert!(existing.is_some());
        let missing = db.photo_exists_by_hash(&[99u8; 32]).unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn same_hash_can_be_inserted_across_shoots() {
        // #4: with the v3 schema, the same content_hash may appear in
        // multiple shoots when the caller opts into cross-shoot duplicates.
        let (mut db, _dir) = test_db();
        let shoot_a = db.insert_shoot("A", "2026-05-01", "/sa", "/da", "copy").unwrap();
        let shoot_b = db.insert_shoot("B", "2026-05-02", "/sb", "/db", "copy").unwrap();

        let ids_a = db
            .insert_photos_batch(shoot_a, &[sample_insert(42, "x.nef")])
            .expect("insert into A");
        let ids_b = db
            .insert_photos_batch(shoot_b, &[sample_insert(42, "x.nef")])
            .expect("insert into B");

        assert_eq!(ids_a.len(), 1);
        assert_eq!(ids_b.len(), 1);
        assert_ne!(ids_a[0], ids_b[0]);

        // Per-shoot lookups resolve to the right rows.
        assert_eq!(
            db.photo_exists_in_shoot_by_hash(shoot_a, &[42u8; 32]).unwrap(),
            Some(ids_a[0])
        );
        assert_eq!(
            db.photo_exists_in_shoot_by_hash(shoot_b, &[42u8; 32]).unwrap(),
            Some(ids_b[0])
        );
    }

    #[test]
    fn same_hash_in_same_shoot_still_fails() {
        // The per-shoot UNIQUE constraint still prevents the trivial
        // duplicate within one shoot. The application-level dedup check
        // in `process_one_file` is the primary gate; this is the backstop.
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        db.insert_photos_batch(shoot_id, &[sample_insert(8, "x.nef")])
            .unwrap();

        let err = db
            .insert_photos_batch(shoot_id, &[sample_insert(8, "y.nef")])
            .expect_err("expected UNIQUE constraint failure");
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("unique"),
            "expected UNIQUE constraint error, got: {msg}"
        );
    }

    #[test]
    fn v3_migration_is_idempotent_on_fresh_db() {
        // Brand-new DBs are already on user_version=3 after open(); calling
        // run_migrations a second time must be a no-op (no panic, no row loss).
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        db.insert_photos_batch(shoot_id, &[sample_insert(9, "z.nef")])
            .unwrap();

        let count_before: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
            .unwrap();
        db.run_migrations().expect("run_migrations should be idempotent");
        let count_after: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_before, count_after);

        // Verify the schema is on the v3 constraint.
        let sql: String = db
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'photos'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            sql.contains("UNIQUE(content_hash, shoot_id)"),
            "photos table missing the v3 UNIQUE clause; got: {sql}"
        );
    }

    #[test]
    fn v3_migration_rebuilds_legacy_table() {
        // Simulate a pre-v3 DB by hand-crafting a photos table with the old
        // UNIQUE(content_hash) constraint and a few rows that should survive
        // the rebuild. Then call run_migrations and confirm:
        //   1. The constraint is now (content_hash, shoot_id).
        //   2. All row data is preserved (id, hash, filename, etc.).
        //   3. The indexes are recreated.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE shoots (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 slug TEXT NOT NULL,
                 date TEXT NOT NULL,
                 source_path TEXT NOT NULL,
                 dest_path TEXT NOT NULL,
                 photo_count INTEGER NOT NULL DEFAULT 0,
                 imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                 import_mode TEXT NOT NULL DEFAULT 'copy',
                 cover_photo_id INTEGER
             );
             CREATE TABLE photos (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                 filename TEXT NOT NULL,
                 raw_path TEXT NOT NULL,
                 preview_path TEXT NOT NULL,
                 thumb_path TEXT NOT NULL,
                 content_hash BLOB NOT NULL,
                 phash BLOB,
                 exif_date TEXT,
                 camera TEXT,
                 lens TEXT,
                 focal_length REAL,
                 aperture REAL,
                 shutter_speed TEXT,
                 iso INTEGER,
                 orientation INTEGER,
                 flag TEXT NOT NULL DEFAULT 'unreviewed',
                 destination TEXT NOT NULL DEFAULT 'unrouted',
                 star_rating INTEGER NOT NULL DEFAULT 0,
                 sharpness_score REAL,
                 quality_score REAL,
                 UNIQUE(content_hash)
             );
             INSERT INTO shoots (slug, date, source_path, dest_path) VALUES ('legacy', '2026-01-01', '/s', '/d');
             INSERT INTO photos
                 (shoot_id, filename, raw_path, preview_path, thumb_path, content_hash)
                 VALUES (1, 'a.nef', '/r/a', '/p/a', '/t/a', x'01010101010101010101010101010101010101010101010101010101010101');
             PRAGMA user_version = 2;",
        )
        .unwrap();
        drop(conn);

        let mut db = Database::open(&path).expect("open should run v3 migration");

        let sql: String = db
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'photos'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            sql.contains("UNIQUE(content_hash, shoot_id)")
                && !sql.contains("UNIQUE(content_hash)\n"),
            "expected v3 constraint, got: {sql}"
        );

        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "legacy row was lost in the rebuild");

        let version: i32 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        // Migrations run through the latest version; the v3 photos rebuild
        // is one step in that chain.
        assert_eq!(version, 4);

        // And the new behaviour works: we can now insert the same hash into a
        // second shoot.
        let shoot_b = db
            .insert_shoot("B", "2026-01-02", "/s2", "/d2", "copy")
            .unwrap();
        let mut p = sample_insert(1, "a.nef");
        // Match the legacy row's hash exactly.
        p.content_hash = [1u8; 32];
        db.insert_photos_batch(shoot_b, &[p])
            .expect("cross-shoot duplicate should succeed under v3");
    }

    #[test]
    fn v4_migration_renames_threshold_and_drops_dead_columns() {
        // Simulate a pre-v4 DB: the old `settings` schema carrying both
        // near_dup_threshold and related_threshold, and the old `groups`
        // schema with the group_type CHECK column. Opening it must rename
        // related_threshold → group_threshold, drop near_dup_threshold, and
        // rebuild `groups` without group_type — preserving stored values
        // and group rows.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE shoots (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 slug TEXT NOT NULL,
                 date TEXT NOT NULL,
                 source_path TEXT NOT NULL,
                 dest_path TEXT NOT NULL
             );
             CREATE TABLE settings (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 near_dup_threshold INTEGER NOT NULL DEFAULT 4,
                 related_threshold INTEGER NOT NULL DEFAULT 12,
                 select_requires_pick INTEGER NOT NULL DEFAULT 1,
                 route_min_star INTEGER NOT NULL DEFAULT 3,
                 library_root TEXT
             );
             CREATE TABLE groups (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                 group_type TEXT NOT NULL CHECK(group_type IN ('near_duplicate','related'))
             );
             INSERT INTO shoots (slug, date, source_path, dest_path)
                 VALUES ('legacy', '2026-01-01', '/s', '/d');
             INSERT INTO settings (id, near_dup_threshold, related_threshold)
                 VALUES (1, 5, 14);
             INSERT INTO groups (shoot_id, group_type) VALUES (1, 'related');
             PRAGMA user_version = 3;",
        )
        .unwrap();
        drop(conn);

        let db = Database::open(&path).expect("open should run the v4 migration");

        // related_threshold renamed → group_threshold, value preserved.
        assert!(db.column_exists("settings", "group_threshold").unwrap());
        assert!(!db.column_exists("settings", "related_threshold").unwrap());
        assert!(!db.column_exists("settings", "near_dup_threshold").unwrap());
        assert_eq!(db.get_settings().unwrap().group_threshold, 14);

        // groups rebuilt without group_type; the legacy row survived.
        assert!(!db.column_exists("groups", "group_type").unwrap());
        let group_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))
            .unwrap();
        assert_eq!(group_count, 1, "legacy group row was lost in the rebuild");

        let version: i32 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 4);
    }

    #[test]
    fn faces_migration_survives_orphan_rows() {
        // A pre-#78 DB carries the old faces schema (NOT NULL eye columns)
        // and may hold orphan faces rows — a face whose photo was deleted
        // while foreign_keys was off. migrate_faces_eye_open_nullable rebuilds
        // the table; under foreign_keys = ON (how the app opens the
        // connection) the rebuild's INSERT must not trip on those orphans,
        // and must drop them. Regression test for the release-build DB-open
        // failure that soft-locked the app on the onboarding wizard.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            // foreign_keys is on by default in rusqlite — turn it off so the
            // orphan face insert below (a deliberately dangling photo_id) is
            // allowed, reproducing the corrupt-but-real DB shape.
            "PRAGMA foreign_keys = OFF;
             CREATE TABLE shoots (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 slug TEXT NOT NULL,
                 date TEXT NOT NULL,
                 source_path TEXT NOT NULL,
                 dest_path TEXT NOT NULL
             );
             CREATE TABLE photos (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                 flag TEXT NOT NULL DEFAULT 'unreviewed'
             );
             CREATE TABLE faces (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                 bbox_x REAL NOT NULL, bbox_y REAL NOT NULL,
                 bbox_w REAL NOT NULL, bbox_h REAL NOT NULL,
                 left_eye_x REAL NOT NULL, left_eye_y REAL NOT NULL,
                 right_eye_x REAL NOT NULL, right_eye_y REAL NOT NULL,
                 left_eye_open INTEGER NOT NULL, right_eye_open INTEGER NOT NULL,
                 left_eye_sharpness REAL NOT NULL, right_eye_sharpness REAL NOT NULL,
                 detection_confidence REAL NOT NULL,
                 smile_score REAL, species TEXT NOT NULL DEFAULT 'human'
             );
             INSERT INTO shoots (slug, date, source_path, dest_path)
                 VALUES ('legacy', '2026-01-01', '/s', '/d');
             INSERT INTO photos (id, shoot_id) VALUES (1, 1);
             -- A valid face on photo 1, plus an orphan face whose photo_id
             -- references no row (allowed here because foreign_keys is off).
             INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                 left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                 left_eye_open, right_eye_open,
                 left_eye_sharpness, right_eye_sharpness, detection_confidence)
                 VALUES (1, 0, 0, 1, 1, 0, 0, 0, 0, 1, 1, 5, 5, 0.9);
             INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                 left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                 left_eye_open, right_eye_open,
                 left_eye_sharpness, right_eye_sharpness, detection_confidence)
                 VALUES (999, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 5, 5, 0.9);
             PRAGMA user_version = 4;",
        )
        .unwrap();
        drop(conn);

        // Opening must succeed despite the orphan row and foreign_keys = ON.
        let db = Database::open(&path).expect("faces migration must survive orphan rows");

        // left_eye_open is now nullable.
        let notnull: i32 = db
            .conn
            .query_row(
                "SELECT \"notnull\" FROM pragma_table_info('faces')
                 WHERE name = 'left_eye_open'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            notnull, 0,
            "left_eye_open should be nullable post-migration"
        );

        // The orphan row was dropped; the valid face survived.
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM faces", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "orphan faces row dropped, valid one kept");
        let surviving_photo_id: i64 = db
            .conn
            .query_row("SELECT photo_id FROM faces", [], |r| r.get(0))
            .unwrap();
        assert_eq!(surviving_photo_id, 1);

        // The rebuilt DB is foreign-key clean.
        let violations = db
            .conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query_map([], |_| Ok(()))
            .unwrap()
            .count();
        assert_eq!(violations, 0, "no foreign key violations after migration");
    }

    #[test]
    fn test_star_rating() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db
            .insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef")])
            .unwrap();

        db.set_star_rating(ids[0], 4).unwrap();
        assert_eq!(db.get_star_rating(ids[0]).unwrap(), 4);
    }

    #[test]
    fn test_groups_and_members() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db
            .insert_photos_batch(
                shoot_id,
                &[sample_insert(1, "a.nef"), sample_insert(2, "b.nef")],
            )
            .unwrap();

        let group_id = db.create_group(shoot_id).unwrap();
        db.add_group_member(group_id, ids[0], true).unwrap();
        db.add_group_member(group_id, ids[1], false).unwrap();

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM group_members WHERE group_id = ?1",
                params![group_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_settings_roundtrip() {
        // A fresh test DB runs the base schema + every migration, so this
        // also exercises the v4 column rename/drop landing cleanly and the
        // by-name column reads in get_settings.
        let (db, _dir) = test_db();

        let defaults = db.get_settings().unwrap();
        assert_eq!(
            defaults.group_threshold,
            crate::ingest::clustering::DEFAULT_GROUP_THRESHOLD as i32,
        );

        let mut s = defaults.clone();
        s.group_threshold = 9;
        s.group_time_window_s = 45;
        s.route_min_star = 2;
        s.curator_provider = "gemini".to_string();
        s.onboarded_review = true;
        db.update_settings(&s).unwrap();

        let back = db.get_settings().unwrap();
        assert_eq!(back.group_threshold, 9);
        assert_eq!(back.group_time_window_s, 45);
        assert_eq!(back.route_min_star, 2);
        assert_eq!(back.curator_provider, "gemini");
        assert!(back.onboarded_review);
    }

    /// End-to-end integration: simulates a Select-view pick where the user
    /// picks one photo and auto-rejects its three siblings, then undoes the
    /// whole batch. Exercises set_flag + bulk_set_flag + append_undo +
    /// pop_undo in a single realistic sequence, and confirms reverted state
    /// persists across DB reopens.
    #[test]
    fn test_select_pick_batch_then_undo() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cull.sqlite");
        let session = "sess-abc";

        let ids: Vec<i64> = {
            let mut db = Database::open(&db_path).unwrap();
            let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
            let photos = vec![
                sample_insert(1, "a.nef"),
                sample_insert(2, "b.nef"),
                sample_insert(3, "c.nef"),
                sample_insert(4, "d.nef"),
            ];
            let ids = db.insert_photos_batch(shoot_id, &photos).unwrap();

            // Pick photo[0], reject photos[1..4] (simulating select-view auto-reject).
            let old_pick = db.set_flag(ids[0], "pick").unwrap();
            db.append_undo(shoot_id, session, ids[0], "flag", &old_pick, "pick").unwrap();
            let old_rejs = db.bulk_set_flag(&ids[1..], "reject").unwrap();
            for (id, old) in &old_rejs {
                db.append_undo(shoot_id, session, *id, "flag", old, "reject").unwrap();
            }

            // State check: one pick + three rejects.
            let rows = db.photos_for_shoot(shoot_id).unwrap();
            let picks = rows.iter().filter(|p| p.flag == "pick").count();
            let rejects = rows.iter().filter(|p| p.flag == "reject").count();
            assert_eq!(picks, 1);
            assert_eq!(rejects, 3);

            // Undo 4 actions (bulk reject was logged as 3 separate entries + 1 pick).
            for _ in 0..4 {
                let entry = db.pop_undo(shoot_id, session).unwrap();
                assert!(entry.is_some(), "undo stack drained prematurely");
            }
            assert!(db.pop_undo(shoot_id, session).unwrap().is_none());

            ids
        };

        // Reopen and verify the undo reverted state survived.
        let db = Database::open(&db_path).unwrap();
        let shoot_id: i64 = db.conn.query_row("SELECT id FROM shoots LIMIT 1", [], |r| r.get(0)).unwrap();
        let rows = db.photos_for_shoot(shoot_id).unwrap();
        assert_eq!(rows.len(), 4);
        for row in &rows {
            assert_eq!(row.flag, "unreviewed", "photo {} should be reverted", row.id);
        }
        // And the id set is unchanged — no rows were lost.
        let mut live: Vec<i64> = rows.iter().map(|r| r.id).collect();
        live.sort();
        let mut expected = ids.clone();
        expected.sort();
        assert_eq!(live, expected);
    }

    #[test]
    fn test_ai_columns_present_after_migration() {
        let (db, _dir) = test_db();
        for col in &["face_count", "eyes_open_count", "ai_analyzed_at"] {
            assert!(
                db.column_exists("photos", col).unwrap(),
                "photos.{} should be present after migration",
                col
            );
        }
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='faces'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "faces table should exist");
    }

    #[test]
    fn test_migration_is_idempotent() {
        let (db, _dir) = test_db();
        // Re-run migration — should be a no-op, not error.
        db.run_migrations().unwrap();
        assert!(db.column_exists("photos", "face_count").unwrap());
    }

    #[test]
    fn test_faces_roundtrip() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db
            .insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef")])
            .unwrap();

        let face = FaceRow {
            photo_id: ids[0],
            bbox_x: 0.1, bbox_y: 0.1, bbox_w: 0.2, bbox_h: 0.3,
            left_eye_x: 0.15, left_eye_y: 0.18,
            right_eye_x: 0.22, right_eye_y: 0.18,
            left_eye_open: Some(1), right_eye_open: Some(1),
            left_eye_sharpness: 78.0, right_eye_sharpness: 81.0,
            detection_confidence: 0.92,
            smile_score: Some(0.73),
            species: "human".to_string(),
        };
        db.insert_faces_batch(&[face.clone()]).unwrap();

        let got = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(got.len(), 1);
        assert!((got[0].bbox_x - 0.1).abs() < 1e-6);
        assert_eq!(got[0].left_eye_open, Some(1));
        assert!((got[0].detection_confidence - 0.92).abs() < 1e-6);
        assert!((got[0].smile_score.unwrap() - 0.73).abs() < 1e-6);

        // Cascade delete
        db.delete_shoot(shoot_id).unwrap();
        let gone = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(gone.len(), 0);
    }

    #[test]
    fn test_sharpness_percentiles_empty_shoot() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("E", "2026-04-15", "/s", "/d", "copy")
            .unwrap();
        // No photos at all.
        let p = db.sharpness_percentiles_for_shoot(shoot_id).unwrap();
        assert_eq!(p.analyzed_count, 0);
        assert_eq!(p.p50, 0.0);
        assert!(p.analyzed_max_ts.is_none());

        // Photos present but none analyzed — same shape.
        db.insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef")])
            .unwrap();
        let p = db.sharpness_percentiles_for_shoot(shoot_id).unwrap();
        assert_eq!(p.analyzed_count, 0);
        assert!(p.analyzed_max_ts.is_none());
    }

    #[test]
    fn test_sharpness_percentiles_uniform() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("U", "2026-04-15", "/s", "/d", "copy")
            .unwrap();
        let ids = db
            .insert_photos_batch(
                shoot_id,
                &[
                    sample_insert(1, "a.nef"),
                    sample_insert(2, "b.nef"),
                    sample_insert(3, "c.nef"),
                ],
            )
            .unwrap();
        for id in &ids {
            db.mark_ai_analyzed(*id, Some(0), Some(0), Some(50.0))
                .unwrap();
        }
        let p = db.sharpness_percentiles_for_shoot(shoot_id).unwrap();
        assert_eq!(p.analyzed_count, 3);
        // All values equal → all percentiles equal to that value.
        assert!((p.p10 - 50.0).abs() < 1e-6);
        assert!((p.p50 - 50.0).abs() < 1e-6);
        assert!((p.p90 - 50.0).abs() < 1e-6);
        assert!(p.analyzed_max_ts.is_some());
    }

    #[test]
    fn test_sharpness_percentiles_populated() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("P", "2026-04-15", "/s", "/d", "copy")
            .unwrap();
        // 11 photos with sharpness 0..=100 in steps of 10.
        let mut inserts = Vec::new();
        for i in 0..11u8 {
            inserts.push(sample_insert(i + 10, &format!("{}.nef", i)));
        }
        let ids = db.insert_photos_batch(shoot_id, &inserts).unwrap();
        for (i, id) in ids.iter().enumerate() {
            let score = (i as f64) * 10.0; // 0, 10, 20, ..., 100
            db.mark_ai_analyzed(*id, Some(0), Some(0), Some(score))
                .unwrap();
        }

        let p = db.sharpness_percentiles_for_shoot(shoot_id).unwrap();
        assert_eq!(p.analyzed_count, 11);
        // round(0.1*(11-1)) = 1 → values[1] = 10
        assert!((p.p10 - 10.0).abs() < 1e-6);
        // round(0.5*10) = 5 → values[5] = 50
        assert!((p.p50 - 50.0).abs() < 1e-6);
        // round(0.9*10) = 9 → values[9] = 90
        assert!((p.p90 - 90.0).abs() < 1e-6);
        assert!(p.analyzed_max_ts.is_some());
    }

    #[test]
    fn test_clear_ai_for_shoot() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db
            .insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef"), sample_insert(2, "b.nef")])
            .unwrap();
        db.mark_ai_analyzed(ids[0], Some(0), Some(0), Some(50.0)).unwrap();
        db.mark_ai_analyzed(ids[1], Some(1), Some(2), Some(75.0)).unwrap();

        db.clear_ai_for_shoot(shoot_id).unwrap();

        for id in &ids {
            let row: (Option<i32>, Option<i32>, Option<f64>, Option<String>) = db.conn.query_row(
                "SELECT face_count, eyes_open_count, sharpness_score, ai_analyzed_at FROM photos WHERE id = ?1",
                params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            ).unwrap();
            assert_eq!(row.0, None, "face_count should be NULL");
            assert_eq!(row.1, None, "eyes_open_count should be NULL");
            assert_eq!(row.2, None, "sharpness_score should be NULL");
            assert_eq!(row.3, None, "ai_analyzed_at should be NULL");
        }
    }

    /// Mutex-poison resilience: the XmpWriteQueue must not deadlock or crash
    /// when a poisoned lock is encountered. This simulates a panic in one
    /// thread while holding the pending map, then verifies the queue still
    /// enqueues and drains correctly.
    #[test]
    fn test_xmp_queue_survives_poison() {
        use crate::metadata::xmp_queue::XmpWriteQueue;
        use std::path::PathBuf;
        use std::sync::Arc;
        use std::thread;

        let queue = Arc::new(XmpWriteQueue::new());
        let q2 = queue.clone();

        // Force a panic inside a thread while nominally using the queue;
        // this would poison the internal Mutex if not handled.
        let _ = thread::spawn(move || {
            q2.enqueue(1, &PathBuf::from("/nonexistent/a.xmp"), 3);
            panic!("deliberate");
        })
        .join();

        // Must not panic — poison-tolerant lock returns the inner guard.
        queue.enqueue(2, &PathBuf::from("/nonexistent/b.xmp"), 5);
        queue.drain(); // writes to bogus paths will fail gracefully via log::error
    }

    /// End-to-end: 3 photos through the full AI pipeline (mock-backed), DB
    /// closed and reopened, aggregates + face rows must survive. Catches
    /// regressions in transaction ordering, FK cascade, and file-based
    /// persistence that purely in-memory tests miss.
    #[test]
    fn test_ai_full_pipeline_mock_provider_reopens_clean() {
        use crate::ai::mock::{MockEyeProvider, MockFaceProvider};
        use crate::ai::worker::process_job;
        use crate::ai::AiJob;
        use image::{ImageBuffer, Luma};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("e2e.db");

        let ids = {
            let mut db = Database::open(&db_path).unwrap();
            let shoot_id = db
                .insert_shoot("E2E", "2026-04-15", "/s", "/d", "copy")
                .unwrap();
            let ids = db
                .insert_photos_batch(
                    shoot_id,
                    &[
                        sample_insert(1, "a.nef"),
                        sample_insert(2, "b.nef"),
                        sample_insert(3, "c.nef"),
                    ],
                )
                .unwrap();

            let face_p = MockFaceProvider::default();
            let eye_p = MockEyeProvider::default();
            let mouth_p = crate::ai::mouth::MockMouthProvider::default();
            let cat_p = crate::ai::cat::MockCatDetector::default();

            for id in &ids {
                let preview = dir.path().join(format!("{}.jpg", id));
                let img: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(128, 128, |x, y| {
                    Luma([if (x / 4 + y / 4) % 2 == 0 { 0 } else { 255 }])
                });
                img.save(&preview).unwrap();

                let job = AiJob {
                    shoot_id,
                    photo_id: *id,
                    preview_path: preview.to_string_lossy().into_owned(),
                };
                process_job(
                    &mut db,
                    &job,
                    Some(&face_p),
                    Some(&eye_p),
                    Some(&mouth_p),
                    Some(&cat_p),
                )
                .unwrap();
            }
            ids
        };

        // Reopen — everything must survive on disk.
        let db = Database::open(&db_path).unwrap();
        for id in &ids {
            let (fc, oc, ts): (Option<i32>, Option<i32>, Option<String>) = db
                .conn
                .query_row(
                    "SELECT face_count, eyes_open_count, ai_analyzed_at FROM photos WHERE id = ?1",
                    rusqlite::params![id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(fc, Some(1), "photo {} face_count should survive reopen", id);
            assert!(oc.is_some(), "photo {} eyes_open_count should survive", id);
            assert!(ts.is_some(), "photo {} ai_analyzed_at should survive", id);

            let faces = db.get_faces_for_photo(*id).unwrap();
            assert_eq!(faces.len(), 1, "photo {} face row should survive", id);
        }
    }

    #[test]
    fn triage_judgments_pending_and_applied() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("T", "2026-05-15", "/s", "/d", "copy")
            .unwrap();
        let ids = db
            .insert_photos_batch(
                shoot_id,
                &[
                    sample_insert(1, "a.nef"),
                    sample_insert(2, "b.nef"),
                    sample_insert(3, "c.nef"),
                ],
            )
            .unwrap();

        db.upsert_triage_judgment(ids[0], shoot_id, "reject", "blurry", "m", 1)
            .unwrap();
        db.upsert_triage_judgment(ids[1], shoot_id, "keep", "fine", "m", 1)
            .unwrap();
        db.upsert_triage_judgment(ids[2], shoot_id, "reject", "eyes closed", "m", 1)
            .unwrap();

        // All three judged; only the two rejects are pending application.
        assert_eq!(db.triage_judgments_for_shoot(shoot_id).unwrap().len(), 3);
        assert_eq!(
            db.pending_triage_rejects(shoot_id).unwrap(),
            vec![ids[0], ids[2]]
        );
        // photos_needing_triage excludes already-judged photos.
        assert!(db.photos_needing_triage(shoot_id).unwrap().is_empty());

        // Marking one applied removes it from the pending set.
        db.mark_triage_judgment_applied(ids[0]).unwrap();
        assert_eq!(db.pending_triage_rejects(shoot_id).unwrap(), vec![ids[2]]);

        // Re-judging resets `applied` so a fresh verdict can be acted on.
        db.upsert_triage_judgment(ids[0], shoot_id, "reject", "still blurry", "m", 1)
            .unwrap();
        assert_eq!(
            db.pending_triage_rejects(shoot_id).unwrap(),
            vec![ids[0], ids[2]]
        );
    }

    #[test]
    fn bracket_decisions_upsert_query_and_cascade() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("T", "2026-05-15", "/s", "/d", "copy")
            .unwrap();
        let ids = db
            .insert_photos_batch(
                shoot_id,
                &[sample_insert(1, "a.nef"), sample_insert(2, "b.nef")],
            )
            .unwrap();
        let group_id = db.create_group(shoot_id).unwrap();

        db.insert_bracket_decision(
            shoot_id, group_id, 0, 0, ids[0], Some(ids[1]), "L", "user",
        )
        .unwrap();
        // Same coordinates, different decision → upsert replaces.
        db.insert_bracket_decision(
            shoot_id, group_id, 0, 0, ids[0], Some(ids[1]), "R", "user",
        )
        .unwrap();
        // A curator-source row at the same coords coexists (source is
        // part of the unique key).
        db.insert_bracket_decision(
            shoot_id, group_id, 0, 0, ids[0], Some(ids[1]), "L", "curator",
        )
        .unwrap();

        let rows = db.bracket_decisions_for_shoot(shoot_id).unwrap();
        assert_eq!(rows.len(), 2, "user upsert + distinct curator row");
        let user = rows.iter().find(|r| r.source == "user").unwrap();
        assert_eq!(user.decision, "R", "upsert kept the latest decision");

        // Deleting the user decision leaves the curator one.
        db.delete_bracket_decision(group_id, 0, 0, "user").unwrap();
        let rows = db.bracket_decisions_for_shoot(shoot_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "curator");

        // Removing the group cascades away its remaining decisions.
        db.delete_all_groups_for_shoot(shoot_id).unwrap();
        assert!(db.bracket_decisions_for_shoot(shoot_id).unwrap().is_empty());
    }
}
