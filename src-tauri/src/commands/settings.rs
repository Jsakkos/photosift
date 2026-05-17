use crate::db::schema::Settings;
use crate::ingest::clustering;
use crate::state::AppState;
use std::sync::Mutex;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, Mutex<AppState>>) -> Result<Settings, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    // Surface the real open failure (e.g. a migration error) rather than a
    // generic message — the frontend renders it on a fatal-error screen.
    let db = app_state.db.as_ref().ok_or_else(|| {
        app_state
            .db_open_error
            .clone()
            .unwrap_or_else(|| "Database not open".to_string())
    })?;
    db.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(
    settings: Settings,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    if !(0..=64).contains(&settings.group_threshold) {
        return Err("group_threshold must be 0..=64".into());
    }
    if settings.route_min_star < 0 || settings.route_min_star > 5 {
        return Err("route_min_star must be 0..=5".into());
    }
    if let Some(root) = settings.library_root.as_deref() {
        if !root.trim().is_empty() {
            let path = std::path::Path::new(root);
            if !path.is_dir() {
                return Err(format!(
                    "library_root is not an existing directory: {}",
                    root
                ));
            }
        }
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    db.update_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn recluster_shoot(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<usize, String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    let settings = {
        let db = app_state.db.as_ref().ok_or("Database not open")?;
        db.get_settings().unwrap_or_default()
    };
    do_recluster(
        &mut app_state,
        shoot_id,
        settings.group_threshold as u32,
        settings.group_time_window_s.max(0) as u32,
    )
}

/// Re-cluster a single shoot with explicit thresholds, bypassing the
/// global `settings` row. Powers the inline regroup control in Select so
/// the user can retune burst grouping for one shoot — the new grouping
/// persists in the `groups` table, while the global defaults are left
/// alone for future imports.
#[tauri::command]
pub fn recluster_shoot_with(
    shoot_id: i64,
    threshold: i32,
    time_window_s: i32,
    state: State<'_, Mutex<AppState>>,
) -> Result<usize, String> {
    if !(0..=64).contains(&threshold) {
        return Err("threshold must be 0..=64".into());
    }
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    do_recluster(&mut app_state, shoot_id, threshold as u32, time_window_s.max(0) as u32)
}

/// Shared re-cluster core: snapshot covers, run the clusterer with the
/// given thresholds, rebuild the shoot's groups. Returns the group count.
fn do_recluster(
    app_state: &mut AppState,
    shoot_id: i64,
    threshold: u32,
    time_window_s: u32,
) -> Result<usize, String> {
    // Snapshot existing covers so we can preserve the user's chosen cover
    // photos when a re-clustered group still contains them.
    let prior_covers: std::collections::HashSet<i64> = {
        let db = app_state.db.as_ref().ok_or("Database not open")?;
        let groups = db.get_groups_for_shoot(shoot_id).map_err(|e| e.to_string())?;
        groups
            .iter()
            .flat_map(|g| g.members.iter().filter(|m| m.is_cover).map(|m| m.photo_id))
            .collect()
    };

    let phash_data = {
        let db = app_state.db.as_ref().ok_or("Database not open")?;
        db.phashes_for_shoot(shoot_id).map_err(|e| e.to_string())?
    };

    let results = clustering::cluster_phashes(&phash_data, threshold, time_window_s);

    let db = app_state.db.as_mut().ok_or("Database not open")?;
    db.delete_all_groups_for_shoot(shoot_id)
        .map_err(|e| e.to_string())?;

    for group in &results {
        let group_id = db
            .create_group(shoot_id)
            .map_err(|e| e.to_string())?;

        // Preserve prior cover if present in the new group; otherwise first member.
        let cover_idx = group
            .member_indices
            .iter()
            .position(|&i| prior_covers.contains(&phash_data[i].0))
            .unwrap_or(0);

        for (i, &idx) in group.member_indices.iter().enumerate() {
            let photo_id = phash_data[idx].0;
            db.add_group_member(group_id, photo_id, i == cover_idx)
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(results.len())
}
