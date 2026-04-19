use crate::db::schema::ShootRow;
use crate::state::AppState;
use std::sync::Mutex;
use tauri::State;

#[tauri::command]
pub fn list_shoots(state: State<'_, Mutex<AppState>>) -> Result<Vec<ShootRow>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    db.list_shoots().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_shoot(shoot_id: i64, state: State<'_, Mutex<AppState>>) -> Result<ShootRow, String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    app_state.load_shoot(shoot_id)?;
    app_state
        .db
        .as_ref()
        .ok_or("Database not open")?
        .get_shoot(shoot_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Shoot not found".into())
}

/// Remove a shoot from the DB (cascades to photos/faces/groups/undo via FK)
/// and wipe its cache directory (previews + thumbs). Canonical copies
/// under the library root are preserved — if the user wants those
/// gone they can delete the folder manually. This keeps re-import
/// cheap since the RAW files stay in place.
#[tauri::command]
pub fn delete_shoot(shoot_id: i64, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    db.delete_shoot(shoot_id).map_err(|e| e.to_string())?;

    // Wipe the on-disk cache. Swallow failures — the DB rows are gone,
    // so orphaned cache files are just disk waste, not a correctness bug.
    let cache_dir = crate::db::schema::shoot_cache_dir(shoot_id);
    if cache_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&cache_dir) {
            log::warn!(
                "delete_shoot: DB row removed but cache wipe failed for {:?}: {}",
                cache_dir,
                e
            );
        }
    }
    Ok(())
}
