use crate::db::schema::shoot_cache_dir;
use crate::metadata::xmp;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

/// Write XMP sidecars for the current shoot.
/// `filter` selects the subset:
///  - "picks"      → all photos with flag = 'pick'
///  - "picks_edit" → picks with destination = 'edit'
///  - "all"        → every photo in the shoot
/// Returns the number of sidecars written.
#[tauri::command]
pub fn export_xmp(
    shoot_id: i64,
    filter: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<usize, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;

    let photos = db.photos_for_shoot(shoot_id).map_err(|e| e.to_string())?;

    let filtered: Vec<_> = photos
        .into_iter()
        .filter(|p| match filter.as_str() {
            "picks" => p.flag == "pick",
            "picks_edit" => p.flag == "pick" && p.destination == "edit",
            "all" => true,
            _ => false,
        })
        .collect();

    let mut written = 0usize;
    for p in &filtered {
        let path = Path::new(&p.raw_path);
        if let Err(e) = xmp::write_cull_metadata(path, p.star_rating, &p.flag, &p.destination) {
            log::error!("XMP export failed for {:?}: {}", path, e);
            continue;
        }
        written += 1;
    }

    Ok(written)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishDirectReport {
    pub copied: usize,
    pub skipped: usize,
    pub failed: usize,
    pub dest_dir: String,
    /// First few failure messages — enough to show the user something
    /// actionable without dumping hundreds of per-photo errors.
    pub errors: Vec<String>,
}

const MAX_REPORT_ERRORS: usize = 5;

/// Copy the cached JPEG preview for every photo whose destination is
/// "publish_direct" into the configured external ingest folder (e.g.
/// Immich's upload directory).
///
/// Errors with "immich_ingest_path not configured" when the setting is
/// unset so the UI can prompt the user. Idempotent: existing files at
/// the destination are skipped, which makes re-running the command safe
/// after a partial failure or a subsequent pick adjustment.
#[tauri::command]
pub fn export_publish_direct(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<PublishDirectReport, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;

    let settings = db.get_settings().map_err(|e| e.to_string())?;
    let dest_dir = settings
        .immich_ingest_path
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "immich_ingest_path not configured".to_string())?
        .clone();
    let dest_dir = PathBuf::from(dest_dir);

    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("cannot create ingest dir {}: {}", dest_dir.display(), e))?;

    let photos = db
        .photos_by_destination(shoot_id, "publish_direct")
        .map_err(|e| e.to_string())?;

    let preview_dir = shoot_cache_dir(shoot_id).join("previews");

    let mut copied = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for p in &photos {
        let src = preview_dir.join(format!("{}.jpg", p.id));
        // Strip any accidental path separators from the stored filename
        // before using it on the destination side — defensive since
        // filenames come from the ingest walker but user-configured dest
        // paths deserve the extra care.
        let safe_stem = Path::new(&p.filename)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("photo_{}", p.id));
        let dest = dest_dir.join(format!("{}.jpg", safe_stem));

        if dest.exists() {
            skipped += 1;
            continue;
        }
        if !src.exists() {
            failed += 1;
            if errors.len() < MAX_REPORT_ERRORS {
                errors.push(format!("missing preview for {} at {}", p.filename, src.display()));
            }
            continue;
        }
        match std::fs::copy(&src, &dest) {
            Ok(_) => copied += 1,
            Err(e) => {
                failed += 1;
                if errors.len() < MAX_REPORT_ERRORS {
                    errors.push(format!("{}: {}", p.filename, e));
                }
            }
        }
    }

    log::info!(
        "publish_direct export shoot={} copied={} skipped={} failed={}",
        shoot_id,
        copied,
        skipped,
        failed
    );

    Ok(PublishDirectReport {
        copied,
        skipped,
        failed,
        dest_dir: dest_dir.to_string_lossy().into_owned(),
        errors,
    })
}
