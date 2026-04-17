use crate::ingest;
use crate::ingest::ImportMode;
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn start_import(
    source_path: String,
    slug: String,
    import_mode: Option<String>,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err("Source path is not a valid directory".into());
    }

    if slug.trim().is_empty() {
        return Err("Slug cannot be empty".into());
    }

    let slug_clean = slug
        .trim()
        .replace(' ', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "");

    let mode = ImportMode::parse(import_mode.as_deref().unwrap_or("copy"));

    let cancel_flag = {
        let app_state = state.lock().map_err(|e| e.to_string())?;
        app_state.import_cancel.store(false, Ordering::Relaxed);
        app_state.import_cancel.clone()
    };

    std::thread::spawn(move || {
        match ingest::run_import(app.clone(), source, slug_clean, mode, cancel_flag) {
            Ok(shoot_id) => {
                log::info!("Import completed: shoot_id={}", shoot_id);
            }
            Err(e) => {
                log::error!("Import failed: {}", e);
                let _ = app.emit("import-error", e);
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_import(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    app_state.import_cancel.store(true, Ordering::Relaxed);
    log::info!("Import cancel requested");
    Ok(())
}
