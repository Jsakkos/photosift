use crate::metadata::xmp;
use crate::state::AppState;
use std::path::Path;
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
