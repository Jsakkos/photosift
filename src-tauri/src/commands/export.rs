use crate::db::schema::shoot_cache_dir;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

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
/// "export" into the configured external ingest folder (e.g. Immich's
/// upload directory).
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
        .photos_by_destination(shoot_id, "export")
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
