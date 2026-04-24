use crate::layout::{sync_shoot_layout, trigger_is_eligible, SyncReport, SyncTrigger};
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

/// Parse the trigger name from the frontend. Kept here rather than on
/// the enum to avoid pulling serde into layout.rs for a string it only
/// uses at the IPC boundary.
fn parse_trigger(name: &str) -> Result<SyncTrigger, String> {
    match name {
        "triage_complete" => Ok(SyncTrigger::TriageComplete),
        "select_complete" => Ok(SyncTrigger::SelectComplete),
        "route_complete" => Ok(SyncTrigger::RouteComplete),
        other => Err(format!("unknown sync trigger: {other}")),
    }
}

/// Frontend calls this on every view transition with the appropriate
/// trigger. The Rust side decides whether the stage's completion
/// criterion is met, and only then runs `sync_shoot_layout`. Returns
/// `None` when gated out so the UI can distinguish "no work to do" from
/// "synced but nothing moved."
#[tauri::command]
pub fn sync_layout_if_eligible(
    shoot_id: i64,
    trigger: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<SyncReport>, String> {
    let trig = parse_trigger(&trigger)?;
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;

    let eligible = trigger_is_eligible(db, shoot_id, trig).map_err(|e| e.to_string())?;
    if !eligible {
        return Ok(None);
    }

    let report = sync_shoot_layout(db, shoot_id)?;
    if !report.errors.is_empty() {
        log::warn!(
            "sync_shoot_layout({shoot_id}, {trigger}) had {} errors: {:?}",
            report.errors.len(),
            report.errors
        );
    }
    Ok(Some(report))
}

#[tauri::command]
pub fn mark_photo_visited_in_select(
    photo_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    db.mark_select_visited(photo_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn bump_select_max_floor(
    shoot_id: i64,
    floor: i32,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    db.bump_select_max_floor(shoot_id, floor)
        .map_err(|e| e.to_string())
}

/// Whitelist the bucket names the UI is allowed to target so these path
/// commands can't be turned into an arbitrary-folder open primitive.
/// Returned as a leaf component only — callers `.join("RAW")` before the
/// leaf so the final PathBuf uses native separators (Windows Explorer
/// silently falls back to Documents when given mixed-separator paths).
fn bucket_leaf(bucket: &str) -> Result<&'static str, String> {
    match bucket {
        "edit" => Ok("edit"),
        "export" => Ok("export"),
        "selects" => Ok("selects"),
        "rejects" => Ok("rejects"),
        other => Err(format!("unknown bucket: {other}")),
    }
}

fn shoot_bucket_path(
    shoot_id: i64,
    bucket: &str,
    state: &State<'_, Mutex<AppState>>,
) -> Result<PathBuf, String> {
    let leaf = bucket_leaf(bucket)?;
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    let shoot = db
        .get_shoot(shoot_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("shoot {shoot_id} not found"))?;
    Ok(PathBuf::from(&shoot.dest_path).join("RAW").join(leaf))
}

/// Resolve the absolute path to a bucket folder within a shoot. Used by
/// the Route view's "Copy path" button so the user can paste it into
/// Capture One's `File → Import Images → Choose Folder` dialog.
#[tauri::command]
pub fn get_shoot_bucket_path(
    shoot_id: i64,
    bucket: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<String, String> {
    let path = shoot_bucket_path(shoot_id, &bucket, &state)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Open a shoot's bucket folder in the system file explorer. This is the
/// "Ship to Capture One / Export" affordance — Capture One on Windows has
/// no import API, so the parity with Narrative Select's "Ship" on Windows
/// is to drop the user at the folder and let them drag it into a session.
///
/// Creates the folder if it doesn't exist yet (a freshly-imported shoot
/// may not have anything routed yet). Returns the absolute path so the
/// UI can also show/copy it.
#[tauri::command]
pub fn open_shoot_folder(
    shoot_id: i64,
    bucket: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<String, String> {
    let path = shoot_bucket_path(shoot_id, &bucket, &state)?;
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("cannot create {}: {}", path.display(), e))?;

    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let opener = "xdg-open";

    log::info!("open_shoot_folder via {}: {}", opener, path.display());
    std::process::Command::new(opener)
        .arg(&path)
        .spawn()
        .map_err(|e| format!("{opener} {}: {}", path.display(), e))?;

    Ok(path.to_string_lossy().into_owned())
}
