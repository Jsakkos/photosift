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
