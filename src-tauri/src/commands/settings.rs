use crate::db::schema::Settings;
use crate::ingest::clustering;
use crate::state::AppState;
use std::sync::Mutex;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, Mutex<AppState>>) -> Result<Settings, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    db.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(
    settings: Settings,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    if settings.near_dup_threshold < 0 || settings.near_dup_threshold > 64 {
        return Err("near_dup_threshold must be 0..=64".into());
    }
    if settings.related_threshold < settings.near_dup_threshold
        || settings.related_threshold > 64
    {
        return Err("related_threshold must be >= near_dup_threshold and <= 64".into());
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

    let settings = {
        let db = app_state.db.as_ref().ok_or("Database not open")?;
        db.get_settings().unwrap_or_default()
    };

    let phash_data = {
        let db = app_state.db.as_ref().ok_or("Database not open")?;
        db.phashes_for_shoot(shoot_id).map_err(|e| e.to_string())?
    };

    let results = clustering::cluster_phashes(
        &phash_data,
        settings.near_dup_threshold as u32,
        settings.related_threshold as u32,
    );

    let db = app_state.db.as_mut().ok_or("Database not open")?;
    db.delete_all_groups_for_shoot(shoot_id)
        .map_err(|e| e.to_string())?;

    for group in &results {
        let group_id = db
            .create_group(shoot_id, group.group_type)
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
