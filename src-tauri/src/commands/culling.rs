use crate::db::schema::{GroupData, UndoEntry};
use crate::state::AppState;
use std::sync::Mutex;
use tauri::State;

const VALID_FLAGS: &[&str] = &["unreviewed", "pick", "reject"];
const VALID_DESTS: &[&str] = &["unrouted", "edit", "publish_direct"];

#[tauri::command]
pub fn set_flag(
    photo_id: i64,
    flag: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<String, String> {
    if !VALID_FLAGS.contains(&flag.as_str()) {
        return Err(format!("Invalid flag: {flag}. Must be one of: {VALID_FLAGS:?}"));
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    let shoot_id = app_state.current_shoot_id.ok_or("No shoot loaded")?;

    let old = db.set_flag(photo_id, &flag).map_err(|e| e.to_string())?;
    let _ = db.append_undo(
        shoot_id,
        &app_state.session_id,
        photo_id,
        "flag",
        &old,
        &flag,
    );

    // Auto-update the shoot cover whenever the pick set changes —
    // promotes the highest-quality pick into the list view so the
    // card reflects what the user actually curated. Non-fatal: a
    // failure here just means the cover stays where it was, which
    // is still a coherent state.
    if flag == "pick" || old == "pick" {
        let _ = db.auto_update_shoot_cover(shoot_id);
    }

    Ok(old)
}

#[tauri::command]
pub fn set_destination(
    photo_id: i64,
    destination: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<String, String> {
    if !VALID_DESTS.contains(&destination.as_str()) {
        return Err(format!(
            "Invalid destination: {destination}. Must be one of: {VALID_DESTS:?}"
        ));
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    let shoot_id = app_state.current_shoot_id.ok_or("No shoot loaded")?;

    let old = db
        .set_destination(photo_id, &destination)
        .map_err(|e| e.to_string())?;
    let _ = db.append_undo(
        shoot_id,
        &app_state.session_id,
        photo_id,
        "destination",
        &old,
        &destination,
    );

    Ok(old)
}

#[tauri::command]
pub fn bulk_set_flag(
    photo_ids: Vec<i64>,
    flag: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    if !VALID_FLAGS.contains(&flag.as_str()) {
        return Err(format!("Invalid flag: {flag}"));
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    let shoot_id = app_state.current_shoot_id.ok_or("No shoot loaded")?;

    let old_values = db
        .bulk_set_flag(&photo_ids, &flag)
        .map_err(|e| e.to_string())?;

    for (pid, old) in &old_values {
        let _ = db.append_undo(shoot_id, &app_state.session_id, *pid, "flag", old, &flag);
    }

    Ok(())
}

#[tauri::command]
pub fn undo_last(state: State<'_, Mutex<AppState>>) -> Result<Option<UndoEntry>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    let shoot_id = app_state.current_shoot_id.ok_or("No shoot loaded")?;

    db.pop_undo(shoot_id, &app_state.session_id)
        .map_err(|e| e.to_string())
}

const VALID_VIEWS: &[&str] = &["triage", "select", "route"];

#[tauri::command]
pub fn get_view_cursor(
    shoot_id: i64,
    view_name: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<i64>, String> {
    if !VALID_VIEWS.contains(&view_name.as_str()) {
        return Err(format!("Invalid view: {view_name}. Must be one of: {VALID_VIEWS:?}"));
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;

    db.get_view_cursor(shoot_id, &view_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_view_cursor(
    shoot_id: i64,
    view_name: String,
    photo_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    if !VALID_VIEWS.contains(&view_name.as_str()) {
        return Err(format!("Invalid view: {view_name}. Must be one of: {VALID_VIEWS:?}"));
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;

    db.set_view_cursor(shoot_id, &view_name, photo_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_groups_for_shoot(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<GroupData>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;

    db.get_groups_for_shoot(shoot_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_group_cover(
    group_id: i64,
    photo_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;

    db.set_group_cover(group_id, photo_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_group_from_photos(
    shoot_id: i64,
    photo_ids: Vec<i64>,
    group_type: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<i64, String> {
    let gt = group_type.unwrap_or_else(|| "near_duplicate".into());
    if gt != "near_duplicate" && gt != "related" {
        return Err(format!("Invalid group_type: {gt}"));
    }
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_mut().ok_or("Database not open")?;

    db.create_group_with_members(shoot_id, &gt, &photo_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ungroup_photos(
    photo_ids: Vec<i64>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_mut().ok_or("Database not open")?;

    db.remove_photos_from_groups(&photo_ids)
        .map_err(|e| e.to_string())
}
