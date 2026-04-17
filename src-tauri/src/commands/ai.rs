use crate::ai::AiProviderStatus;
use crate::db::schema::FaceRow;
use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub provider: AiProviderStatus,
    pub analyzed: usize,
    pub failed: usize,
    pub total: usize,
}

#[tauri::command]
pub fn get_ai_status(state: State<'_, Mutex<AppState>>) -> Result<AiStatus, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(AiStatus {
        provider: s.ai_status,
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
