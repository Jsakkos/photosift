//! Debug-only commands for screenshot CI and integration tests.
//!
//! Compiled and registered ONLY under `#[cfg(debug_assertions)]` — release
//! builds cannot call these. The whole module is intentionally hidden
//! behind the cfg so the surface stays exactly zero in production.
//!
//! The dance:
//!   1. Test runner sets `PHOTOSIFT_HOME=<tempdir>` (and optionally
//!      `PHOTOSIFT_E2E_FIXTURES=<repo>/tests/e2e/fixtures/img`) before
//!      spawning the binary.
//!   2. Test calls `seed_test_fixtures` with a generic `SeedRequest`
//!      describing shoots/photos/faces/judgments/settings overrides.
//!   3. This module truncates the global DB, inserts the requested rows
//!      via the same typed CRUD that production uses, and copies each
//!      photo's preview/thumb JPEG from the fixtures dir to the
//!      `$PHOTOSIFT_HOME/cache/{shoot_id}/{previews,thumbs}/` path the
//!      `photosift://` protocol handler reads from.
//!   4. For ephemeral AppState fields that aren't DB-backed (curator
//!      status, the "currently running" shoot id badge), the test calls
//!      `set_screenshot_state` to pin them synchronously.

#![cfg(debug_assertions)]

use crate::curator::CuratorStatus;
use crate::db::schema::{photosift_home, shoot_cache_dir, Database, FaceRow, PhotoInsert};
use crate::state::AppState;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

/// The contract the JS-side fixture builders serialize to. Fully data —
/// no fixture *names* land in Rust, so adding a new fixture is purely
/// a JS-side change.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedRequest {
    /// Wipe all tables before seeding. Defaults to true; set false to
    /// layer additional rows onto an existing fixture state.
    #[serde(default = "default_true")]
    pub truncate: bool,
    pub shoots: Vec<SeedShoot>,
    /// Per-key patches over the `Settings` row. Anything left None
    /// preserves the current value (which is `Settings::default()` after
    /// a truncate).
    #[serde(default)]
    pub settings: Option<SeedSettings>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedShoot {
    pub slug: String,
    pub date: String,
    pub photos: Vec<SeedPhoto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedPhoto {
    /// Filename of the source preview JPEG under `PHOTOSIFT_E2E_FIXTURES`
    /// (e.g. `"sample_01.jpg"`). The same image is reused for thumb +
    /// preview at the cache paths the protocol handler reads.
    pub fixture: String,
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub star_rating: Option<i32>,
    #[serde(default)]
    pub camera: Option<String>,
    #[serde(default)]
    pub lens: Option<String>,
    #[serde(default)]
    pub exif_date: Option<String>,
    #[serde(default)]
    pub sharpness_score: Option<f64>,
    #[serde(default)]
    pub quality_score: Option<f64>,
    #[serde(default)]
    pub face_count: Option<i32>,
    #[serde(default)]
    pub eyes_open_count: Option<i32>,
    #[serde(default)]
    pub faces: Vec<SeedFace>,
    #[serde(default)]
    pub curator_judgment: Option<SeedCuratorJudgment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedFace {
    pub bbox_x: f64,
    pub bbox_y: f64,
    pub bbox_w: f64,
    pub bbox_h: f64,
    #[serde(default = "default_eye_open")]
    pub left_eye_open: i32,
    #[serde(default = "default_eye_open")]
    pub right_eye_open: i32,
    #[serde(default)]
    pub smile_score: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedCuratorJudgment {
    pub composition: i32,
    pub aesthetic: i32,
    pub is_keeper: bool,
    pub suggested_flag: String,
    #[serde(default = "default_reason")]
    pub reason: String,
    #[serde(default = "default_provider")]
    pub provider: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeedSettings {
    pub onboarded_triage: Option<bool>,
    pub onboarded_select: Option<bool>,
    pub onboarded_route: Option<bool>,
    pub onboarded_wizard: Option<bool>,
    pub route_min_star: Option<i32>,
    pub select_requires_pick: Option<bool>,
    pub library_root: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_eye_open() -> i32 {
    1
}
fn default_reason() -> String {
    "Test fixture".into()
}
fn default_provider() -> String {
    "anthropic".into()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedResult {
    pub shoot_ids: Vec<i64>,
    pub photo_ids: Vec<Vec<i64>>,
}

#[tauri::command]
pub fn seed_test_fixtures(
    request: SeedRequest,
    state: State<'_, Mutex<AppState>>,
) -> Result<SeedResult, String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;

    if app_state.db.is_none() {
        let path = crate::db::schema::global_db_path();
        let db = Database::open(&path).map_err(|e| format!("open db: {e}"))?;
        app_state.db = Some(db);
    }

    if request.truncate {
        let db = app_state
            .db
            .as_ref()
            .ok_or("Database not open after open()")?;
        truncate_all(db).map_err(|e| format!("truncate: {e}"))?;
        let cache_root = photosift_home().join("cache");
        if cache_root.exists() {
            let _ = std::fs::remove_dir_all(&cache_root);
        }
    }

    if let Some(s) = &request.settings {
        apply_settings_overrides(app_state.db.as_ref().unwrap(), s)
            .map_err(|e| format!("settings: {e}"))?;
    }

    let fixtures_dir = fixtures_root()?;

    let mut shoot_ids = Vec::with_capacity(request.shoots.len());
    let mut all_photo_ids: Vec<Vec<i64>> = Vec::with_capacity(request.shoots.len());

    for shoot in &request.shoots {
        let shoot_id = insert_shoot_row(app_state.db.as_ref().unwrap(), shoot)?;
        shoot_ids.push(shoot_id);

        let cache = shoot_cache_dir(shoot_id);
        let preview_dir = cache.join("previews");
        let thumb_dir = cache.join("thumbs");
        std::fs::create_dir_all(&preview_dir).map_err(|e| format!("mkdir previews: {e}"))?;
        std::fs::create_dir_all(&thumb_dir).map_err(|e| format!("mkdir thumbs: {e}"))?;

        let photo_inserts: Vec<PhotoInsert> = shoot
            .photos
            .iter()
            .enumerate()
            .map(|(idx, p)| photo_insert_for(idx, shoot_id, p, &preview_dir, &thumb_dir))
            .collect();

        let photo_ids = app_state
            .db
            .as_mut()
            .unwrap()
            .insert_photos_batch(shoot_id, &photo_inserts)
            .map_err(|e| format!("insert photos: {e}"))?;

        for (photo_id, seed) in photo_ids.iter().zip(shoot.photos.iter()) {
            let src = fixtures_dir.join(&seed.fixture);
            let preview_dest = preview_dir.join(format!("{photo_id}.jpg"));
            let thumb_dest = thumb_dir.join(format!("{photo_id}.jpg"));
            std::fs::copy(&src, &preview_dest).map_err(|e| {
                format!(
                    "copy preview from {} -> {}: {}",
                    src.display(),
                    preview_dest.display(),
                    e
                )
            })?;
            std::fs::copy(&src, &thumb_dest).map_err(|e| {
                format!("copy thumb: {e}")
            })?;

            app_state
                .db
                .as_ref()
                .unwrap()
                .update_photo_paths(
                    *photo_id,
                    &preview_dest.to_string_lossy(),
                    &thumb_dest.to_string_lossy(),
                )
                .map_err(|e| format!("update paths: {e}"))?;

            apply_ai_aggregates(app_state.db.as_ref().unwrap(), *photo_id, seed)?;

            if !seed.faces.is_empty() {
                let face_rows: Vec<FaceRow> = seed
                    .faces
                    .iter()
                    .map(|f| face_row_for(*photo_id, f))
                    .collect();
                app_state
                    .db
                    .as_mut()
                    .unwrap()
                    .insert_faces_batch(&face_rows)
                    .map_err(|e| format!("faces: {e}"))?;
            }

            if let Some(j) = &seed.curator_judgment {
                app_state
                    .db
                    .as_ref()
                    .unwrap()
                    .upsert_curator_judgment(
                        *photo_id,
                        shoot_id,
                        j.composition,
                        j.aesthetic,
                        None,
                        j.is_keeper,
                        &j.suggested_flag,
                        &j.reason,
                        &j.provider,
                        "screenshot-fixture",
                        1,
                    )
                    .map_err(|e| format!("judgment: {e}"))?;
            }
        }

        app_state
            .db
            .as_ref()
            .unwrap()
            .update_shoot_photo_count(shoot_id, shoot.photos.len() as i64)
            .map_err(|e| format!("update photo count: {e}"))?;
        if let Some(first) = photo_ids.first() {
            app_state
                .db
                .as_ref()
                .unwrap()
                .set_shoot_cover_if_unset(shoot_id, *first)
                .map_err(|e| format!("set cover: {e}"))?;
        }

        all_photo_ids.push(photo_ids);
    }

    Ok(SeedResult {
        shoot_ids,
        photo_ids: all_photo_ids,
    })
}

fn fixtures_root() -> Result<PathBuf, String> {
    let raw = std::env::var("PHOTOSIFT_E2E_FIXTURES").map_err(|_| {
        "PHOTOSIFT_E2E_FIXTURES env var not set — testing commands require it to locate fixture JPEGs".to_string()
    })?;
    let p = PathBuf::from(raw);
    if !p.is_dir() {
        return Err(format!(
            "PHOTOSIFT_E2E_FIXTURES does not point at a directory: {}",
            p.display()
        ));
    }
    Ok(p)
}

fn truncate_all(db: &Database) -> rusqlite::Result<()> {
    db.conn.execute_batch(
        "DELETE FROM faces;
         DELETE FROM curator_judgments;
         DELETE FROM undo_log;
         DELETE FROM view_cursors;
         DELETE FROM group_members;
         DELETE FROM groups;
         DELETE FROM file_moves;
         DELETE FROM photos;
         DELETE FROM shoots;
         DELETE FROM sqlite_sequence
             WHERE name IN ('shoots','photos','faces','groups','curator_judgments','undo_log','file_moves');
         DELETE FROM settings;
         INSERT INTO settings (id) VALUES (1);",
    )?;
    Ok(())
}

fn insert_shoot_row(db: &Database, shoot: &SeedShoot) -> Result<i64, String> {
    let dest_path = photosift_home()
        .join("library")
        .join(&shoot.slug)
        .to_string_lossy()
        .into_owned();
    db.insert_shoot(
        &shoot.slug,
        &shoot.date,
        "fixture://source",
        &dest_path,
        "copy",
    )
    .map_err(|e| format!("insert_shoot: {e}"))
}

fn photo_insert_for(
    idx: usize,
    shoot_id: i64,
    seed: &SeedPhoto,
    preview_dir: &std::path::Path,
    thumb_dir: &std::path::Path,
) -> PhotoInsert {
    let filename = format!("DSC_{:04}.JPG", idx + 1);
    let placeholder_preview = preview_dir.join("placeholder.jpg");
    let placeholder_thumb = thumb_dir.join("placeholder.jpg");
    let mut content_hash = [0u8; 32];
    content_hash[..8].copy_from_slice(&(shoot_id as u64).to_le_bytes());
    content_hash[8..16].copy_from_slice(&(idx as u64).to_le_bytes());
    PhotoInsert {
        filename,
        raw_path: format!("fixture://{}-{}", shoot_id, idx + 1),
        preview_path: placeholder_preview.to_string_lossy().into_owned(),
        thumb_path: placeholder_thumb.to_string_lossy().into_owned(),
        content_hash,
        phash: Some([(idx as u8); 8]),
        exif_date: seed.exif_date.clone().or_else(|| {
            let h = 10 + (idx as u32) / 60;
            let m = (idx as u32) % 60;
            Some(format!("2026-04-15T{:02}:{:02}:00", h % 24, m))
        }),
        camera: seed
            .camera
            .clone()
            .or_else(|| Some("NIKON D750".to_string())),
        lens: seed
            .lens
            .clone()
            .or_else(|| Some("50mm f/1.8".to_string())),
        focal_length: Some(50.0),
        aperture: Some(1.8),
        shutter_speed: Some("1/250".into()),
        iso: Some(400),
        orientation: Some(1),
        file_size_bytes: Some(1024 * 1024),
        initial_flag: seed.flag.clone(),
        initial_star_rating: seed.star_rating,
        sidecar_jpeg_path: None,
    }
}

fn apply_ai_aggregates(
    db: &Database,
    photo_id: i64,
    seed: &SeedPhoto,
) -> Result<(), String> {
    if seed.face_count.is_none()
        && seed.eyes_open_count.is_none()
        && seed.sharpness_score.is_none()
        && seed.quality_score.is_none()
        && seed.destination.is_none()
    {
        return Ok(());
    }

    let dest = seed.destination.as_deref().unwrap_or("unrouted");
    db.conn
        .execute(
            "UPDATE photos
             SET face_count = COALESCE(?2, face_count),
                 eyes_open_count = COALESCE(?3, eyes_open_count),
                 sharpness_score = COALESCE(?4, sharpness_score),
                 quality_score = COALESCE(?5, quality_score),
                 ai_analyzed_at = CASE WHEN ?2 IS NOT NULL OR ?4 IS NOT NULL
                                       THEN datetime('now') ELSE ai_analyzed_at END,
                 destination = ?6
             WHERE id = ?1",
            rusqlite::params![
                photo_id,
                seed.face_count,
                seed.eyes_open_count,
                seed.sharpness_score,
                seed.quality_score,
                dest,
            ],
        )
        .map_err(|e| format!("update ai aggregates: {e}"))?;
    Ok(())
}

fn face_row_for(photo_id: i64, f: &SeedFace) -> FaceRow {
    FaceRow {
        photo_id,
        bbox_x: f.bbox_x,
        bbox_y: f.bbox_y,
        bbox_w: f.bbox_w,
        bbox_h: f.bbox_h,
        left_eye_x: f.bbox_x + f.bbox_w * 0.3,
        left_eye_y: f.bbox_y + f.bbox_h * 0.35,
        right_eye_x: f.bbox_x + f.bbox_w * 0.7,
        right_eye_y: f.bbox_y + f.bbox_h * 0.35,
        left_eye_open: f.left_eye_open,
        right_eye_open: f.right_eye_open,
        left_eye_sharpness: 1.0,
        right_eye_sharpness: 1.0,
        detection_confidence: 0.95,
        smile_score: f.smile_score,
        species: "human".to_string(),
    }
}

fn apply_settings_overrides(db: &Database, s: &SeedSettings) -> Result<(), String> {
    let mut current = db
        .get_settings()
        .map_err(|e| format!("get settings: {e}"))?;
    if let Some(v) = s.onboarded_triage {
        current.onboarded_triage = v;
    }
    if let Some(v) = s.onboarded_select {
        current.onboarded_select = v;
    }
    if let Some(v) = s.onboarded_route {
        current.onboarded_route = v;
    }
    if let Some(v) = s.onboarded_wizard {
        current.onboarded_wizard = v;
    }
    if let Some(v) = s.route_min_star {
        current.route_min_star = v;
    }
    if let Some(v) = s.select_requires_pick {
        current.select_requires_pick = v;
    }
    if let Some(v) = s.library_root.clone() {
        current.library_root = Some(v);
    }
    db.update_settings(&current)
        .map_err(|e| format!("update settings: {e}"))
}

// ----- set_screenshot_state -----

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotStatePatch {
    /// `"idle" | "running" | "failed" | "disabled"`.
    pub curator_status: Option<String>,
    /// `Some(Some(id))` sets the badge to a specific shoot,
    /// `Some(None)` clears the badge, `None` leaves the field alone.
    /// Encoded as a tagged variant on the JS side because of the
    /// option-of-option ambiguity in JSON.
    pub curator_running_shoot_id: Option<CuratorRunningPatch>,
    /// Reset the curator progress counters to a fixed (processed, failed, total) tuple
    /// so the percentage in any UI badge is deterministic.
    pub curator_progress: Option<CuratorProgressPatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CuratorRunningPatch {
    Clear,
    Set(i64),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CuratorProgressPatch {
    pub processed: usize,
    pub failed: usize,
    pub total: usize,
}

#[tauri::command]
pub fn set_screenshot_state(
    patch: ScreenshotStatePatch,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let mut app_state = state.lock().map_err(|e| e.to_string())?;

    if let Some(s) = patch.curator_status.as_deref() {
        app_state.curator_status = match s {
            "idle" => CuratorStatus::Idle,
            "running" => CuratorStatus::Running,
            "failed" => CuratorStatus::Failed,
            "disabled" => CuratorStatus::Disabled,
            other => return Err(format!("unknown curator status: {other}")),
        };
    }

    if let Some(p) = patch.curator_running_shoot_id {
        app_state.curator_running_shoot_id = match p {
            CuratorRunningPatch::Clear => None,
            CuratorRunningPatch::Set(id) => Some(id),
        };
    }

    if let Some(p) = patch.curator_progress {
        app_state
            .curator_processed
            .store(p.processed, Ordering::SeqCst);
        app_state.curator_failed.store(p.failed, Ordering::SeqCst);
        app_state.curator_total.store(p.total, Ordering::SeqCst);
    }

    Ok(())
}
