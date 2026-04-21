use crate::ai::sharpness::tiled_tenengrad;
use crate::ai::{AiProviderStatus, EyeProviderKind, MouthProviderKind};
use crate::db::schema::{FaceRow, SharpnessPercentiles};
use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub provider: AiProviderStatus,
    pub eye_provider: EyeProviderKind,
    pub mouth_provider: MouthProviderKind,
    pub analyzed: usize,
    pub failed: usize,
    pub total: usize,
}

#[tauri::command]
pub fn get_ai_status(state: State<'_, Mutex<AppState>>) -> Result<AiStatus, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(AiStatus {
        provider: s.ai_status,
        eye_provider: s.ai_eye_provider,
        mouth_provider: s.ai_mouth_provider,
        analyzed: s.ai_analyzed.load(Ordering::SeqCst),
        failed: s.ai_failed.load(Ordering::SeqCst),
        total: s.ai_total.load(Ordering::SeqCst),
    })
}

#[tauri::command]
pub fn cancel_ai_analysis(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.ai_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn reanalyze_shoot(
    state: State<'_, Mutex<AppState>>,
    shoot_id: i64,
) -> Result<(), String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    // clear_ai_for_shoot takes &mut self.
    {
        let db = s.db.as_mut().ok_or("db not open")?;
        db.clear_ai_for_shoot(shoot_id).map_err(|e| e.to_string())?;
    }
    let ids = {
        let db = s.db.as_ref().ok_or("db not open")?;
        db.photos_needing_ai(shoot_id).map_err(|e| e.to_string())?
    };
    let worker = s.ai_worker.as_ref().ok_or("ai worker not running")?;
    let base_dir = crate::db::schema::shoot_cache_dir(shoot_id).join("previews");
    for id in &ids {
        let preview = base_dir.join(format!("{}.jpg", id));
        let _ = worker.sender.send(crate::ai::AiJob {
            shoot_id,
            photo_id: *id,
            preview_path: preview.to_string_lossy().into_owned(),
        });
    }
    s.ai_total.fetch_add(ids.len(), Ordering::SeqCst);
    // Reset cancel flag in case the user previously cancelled.
    s.ai_cancel.store(false, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn get_faces_for_photo(
    state: State<'_, Mutex<AppState>>,
    photo_id: i64,
) -> Result<Vec<FaceRow>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    db.get_faces_for_photo(photo_id).map_err(|e| e.to_string())
}

/// Returns the current shoot's sharpness percentile cutoffs. The frontend
/// maps raw sharpness scores to a 1-10 scale using these buckets so the UI
/// stays informative across shoots with different raw-variance profiles.
///
/// Caches on `(shoot_id, MAX(ai_analyzed_at))`: since ai_analyzed_at only
/// advances monotonically as analysis results land, the cached entry is
/// stale iff the shoot has new analyzed photos, and no explicit busting
/// from the caller is needed.
#[tauri::command]
pub fn get_shoot_sharpness_percentiles(
    state: State<'_, Mutex<AppState>>,
    shoot_id: i64,
) -> Result<SharpnessPercentiles, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;

    // Quick probe of the current max timestamp so we can compare against
    // the cached entry without doing the full percentile SQL scan yet.
    let latest_ts: Option<String> = db
        .conn
        .query_row(
            "SELECT MAX(ai_analyzed_at) FROM photos
             WHERE shoot_id = ?1 AND ai_analyzed_at IS NOT NULL",
            rusqlite::params![shoot_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .unwrap_or(None);

    if let Some((cached_shoot, cached_ts, cached)) = &s.percentile_cache {
        if *cached_shoot == shoot_id && *cached_ts == latest_ts {
            return Ok(cached.clone());
        }
    }

    let fresh = db
        .sharpness_percentiles_for_shoot(shoot_id)
        .map_err(|e| e.to_string())?;
    log::debug!(
        "ai::percentiles shoot={} count={} p10={:.2} p30={:.2} p50={:.2} p70={:.2} p90={:.2}",
        shoot_id,
        fresh.analyzed_count,
        fresh.p10,
        fresh.p30,
        fresh.p50,
        fresh.p70,
        fresh.p90,
    );
    s.percentile_cache = Some((shoot_id, latest_ts, fresh.clone()));
    Ok(fresh)
}

#[tauri::command]
pub fn get_heatmap(
    state: State<'_, Mutex<AppState>>,
    photo_id: i64,
) -> Result<Vec<f64>, String> {
    // Snapshot the path under the lock, then drop it before doing I/O.
    let preview_path = {
        let s = state.lock().map_err(|e| e.to_string())?;
        let db = s.db.as_ref().ok_or("db not open")?;
        let photo = db.get_photo_by_id(photo_id).map_err(|e| e.to_string())?;
        photo.preview_path.clone()
    };
    let img = image::open(&preview_path).map_err(|e| e.to_string())?;
    let gray = img.to_luma8();
    // 48 cols × 32 rows: finer horizontal resolution to match a 3:2
    // frame. Tenengrad (Sobel gradient magnitude) replaces the former
    // Laplacian variance because variance conflates "low-frequency
    // texture" with "soft focus" — visibly-in-focus foliage scored
    // identically to a blurred wall. Gradient magnitude tracks edge
    // strength directly.
    let grid = tiled_tenengrad(&gray, 48, 32);

    // Per-image normalization: map `[p5, p95]` of this photo's tile
    // scores to `[0, 100]`. Using 5th/95th percentiles rather than
    // raw min/max so a single very-dark or very-bright tile doesn't
    // collapse the rest of the range. This decouples the heatmap from
    // the absolute `CALIBRATION_FULL_SCALE` used for sort/filter.
    let (lo, hi) = percentile_range(&grid, 0.05, 0.95);
    log::debug!(
        "ai::heatmap photo={} grid_min={:.2} p5={:.2} p95={:.2} grid_max={:.2}",
        photo_id,
        grid.iter().cloned().fold(f64::INFINITY, f64::min),
        lo,
        hi,
        grid.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );

    let span = (hi - lo).max(1e-6);
    Ok(grid
        .into_iter()
        .map(|v| {
            let t = ((v - lo) / span).clamp(0.0, 1.0);
            t * 100.0
        })
        .collect())
}

fn percentile_range(values: &[f64], lo: f64, hi: f64) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 1.0);
    }
    let mut sorted: Vec<f64> = values.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = |q: f64| {
        let i = (q * (sorted.len() as f64 - 1.0)).round() as usize;
        sorted[i.min(sorted.len() - 1)]
    };
    (idx(lo), idx(hi))
}
