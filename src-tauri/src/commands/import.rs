use crate::ingest;
use crate::ingest::ImportMode;
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn start_import(
    source_path: String,
    slug: String,
    import_mode: Option<String>,
    // Optional per-file allow-list from the pre-import scan dialog. `None`
    // falls back to the pre-C2 behavior of importing every supported file
    // under `source_path`.
    selected_paths: Option<Vec<String>>,
    // When set, files are added to this existing shoot (#12) instead of
    // creating a new one — the slug/import_mode args are then ignored in
    // favour of the shoot's recorded values.
    existing_shoot_id: Option<i64>,
    // ImportDialog's "Skip duplicates" checkbox (#4). `None` defaults to
    // true so existing IPC callers (tests, scripted runs) keep the
    // historical global-dedup behaviour.
    skip_duplicates: Option<bool>,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err("Source path is not a valid directory".into());
    }

    // The slug is only used when creating a fresh shoot.
    if existing_shoot_id.is_none() && slug.trim().is_empty() {
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

    let selected = selected_paths.map(|v| v.into_iter().map(PathBuf::from).collect());
    let skip_duplicates = skip_duplicates.unwrap_or(true);

    // Clone state handle for the post-import hook.
    let app_for_ai = app.clone();
    std::thread::spawn(move || {
        match ingest::run_import(
            app.clone(),
            source,
            slug_clean,
            mode,
            cancel_flag,
            selected,
            existing_shoot_id,
            skip_duplicates,
        ) {
            Ok(shoot_id) => {
                log::info!("Import completed: shoot_id={}", shoot_id);
                enqueue_ai_for_shoot(&app_for_ai, shoot_id);
                // Triage runs after the AI jobs are queued so most local
                // signals are usually ready by the time the LLM sees a
                // frame — but it never blocks on AI completion.
                enqueue_curator_triage_for_shoot(&app_for_ai, shoot_id);
            }
            Err(e) => {
                log::error!("Import failed: {}", e);
                let _ = app.emit("import-error", e);
            }
        }
    });

    Ok(())
}

/// Preflight for "add to existing shoot": derive the YYYY-MM of the source
/// folder's first photo so the UI can warn before committing when it doesn't
/// match the target shoot's month.
#[tauri::command]
pub fn derive_import_year_month(source_path: String) -> Result<String, String> {
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err("Source path is not a valid directory".into());
    }
    let items = ingest::pairing::pair(ingest::walker::walk_source(&source));
    let first = items
        .first()
        .ok_or_else(|| "No supported image files found in source directory".to_string())?;
    Ok(ingest::derive_yyyy_mm(first.primary_path()))
}

#[tauri::command]
pub fn cancel_import(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    app_state.import_cancel.store(true, Ordering::Relaxed);
    log::info!("Import cancel requested");
    Ok(())
}

/// After a successful import, enqueue all new photos into the AI worker
/// (if the user has `enable_ai_on_import` on and the worker is running).
fn enqueue_ai_for_shoot(app: &AppHandle, shoot_id: i64) {
    let state: tauri::State<'_, Mutex<AppState>> = app.state::<Mutex<AppState>>();
    let guard = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            log::error!("enqueue_ai_for_shoot: state lock poisoned: {}", e);
            return;
        }
    };

    let settings = match guard.db.as_ref().and_then(|db| db.get_settings().ok()) {
        Some(s) => s,
        None => {
            log::warn!("enqueue_ai_for_shoot: no settings — skipping");
            return;
        }
    };
    if !settings.enable_ai_on_import {
        log::info!("AI-on-import disabled; skipping enqueue for shoot {}", shoot_id);
        return;
    }

    let ids = match guard.db.as_ref().and_then(|db| db.photos_needing_ai(shoot_id).ok()) {
        Some(v) => v,
        None => {
            log::warn!("enqueue_ai_for_shoot: photos_needing_ai failed for shoot {}", shoot_id);
            return;
        }
    };

    let worker = match guard.ai_worker.as_ref() {
        Some(w) => w,
        None => {
            log::warn!("enqueue_ai_for_shoot: no worker running");
            return;
        }
    };

    // Reset the cancel flag FIRST so any stale cancel from a prior
    // session doesn't cause the worker to drop jobs we're about to
    // enqueue. (The worker's own loop also clears it on cancel now,
    // but this belt-and-suspenders ensures the race window is closed.)
    guard.ai_cancel.store(false, Ordering::SeqCst);

    let base = crate::db::schema::shoot_cache_dir(shoot_id).join("previews");
    let mut sent = 0_usize;
    let mut send_errors = 0_usize;
    for id in &ids {
        let preview = base.join(format!("{}.jpg", id));
        match worker.sender.send(crate::ai::AiJob {
            shoot_id,
            photo_id: *id,
            preview_path: preview.to_string_lossy().into_owned(),
        }) {
            Ok(()) => sent += 1,
            Err(_) => send_errors += 1,
        }
    }
    if send_errors > 0 {
        log::error!(
            "enqueue_ai_for_shoot: {} jobs failed to send (worker thread likely exited)",
            send_errors
        );
    }
    guard.ai_total.fetch_add(sent, Ordering::SeqCst);
    log::info!("Enqueued {} photos for AI analysis in shoot {}", sent, shoot_id);
}

/// After a successful import, enqueue all new photos into the Curator
/// triage stage — an on-import LLM first pass that auto-rejects
/// technically-unusable frames. Best-effort: a disabled toggle or a
/// missing provider key just skips triage; import already succeeded.
fn enqueue_curator_triage_for_shoot(app: &AppHandle, shoot_id: i64) {
    let state: tauri::State<'_, Mutex<AppState>> = app.state::<Mutex<AppState>>();

    let settings = {
        let guard = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::error!("enqueue_curator_triage: state lock poisoned: {}", e);
                return;
            }
        };
        match guard.db.as_ref().and_then(|db| db.get_settings().ok()) {
            Some(s) => s,
            None => return,
        }
    };
    if !settings.curator_triage_on_import {
        log::info!("Curator triage-on-import disabled; skipping shoot {}", shoot_id);
        return;
    }

    // Spawn the shared curator worker. Fails when no provider key is
    // configured — triage then silently no-ops, the intended behaviour
    // for the default-on toggle.
    if let Err(e) = crate::commands::curator::ensure_worker_spawned(app, &state) {
        log::info!("Curator triage skipped (no provider configured): {}", e);
        return;
    }

    let guard = match state.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let ids = match guard
        .db
        .as_ref()
        .and_then(|db| db.photos_needing_triage(shoot_id).ok())
    {
        Some(v) if !v.is_empty() => v,
        _ => return,
    };
    let sender = match guard.curator_worker.as_ref() {
        Some(w) => w.sender.clone(),
        None => return,
    };
    // Clear any stale cancel flag so the worker doesn't drain our jobs.
    guard.curator_cancel.store(false, Ordering::SeqCst);
    drop(guard);

    let mut sent = 0usize;
    for chunk in ids.chunks(crate::curator::worker::TRIAGE_BATCH_SIZE) {
        if sender
            .send(crate::curator::CuratorJob::TriageBatch {
                shoot_id,
                photo_ids: chunk.to_vec(),
            })
            .is_ok()
        {
            sent += 1;
        }
    }
    log::info!(
        "Enqueued {} curator triage batches for shoot {}",
        sent,
        shoot_id
    );
}
