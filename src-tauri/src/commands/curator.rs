//! Tauri command surface for the AI Curator subsystem. Three groups:
//!
//! 1. API key management — generic `set/clear/get_curator_api_key` plus
//!    `test_curator_connection`. Anthropic-named shims are kept for
//!    backwards compatibility with the existing frontend.
//! 2. Run control — `start_curator_for_shoot`, `cancel_curator`,
//!    `resume_curator_for_shoot`, `clear_curator_for_shoot`.
//! 3. Read accessors — `get_curator_status`, `get_curator_judgment_for_photo`,
//!    `get_curator_summary`, `get_curator_agreement_stats`, plus
//!    `accept_curator_suggestion` (writes flag + user_action atomically).
//!
//! Worker lifecycle: `start_curator_for_shoot` lazily spawns the worker
//! the first time it's called, threading the API key from the keychain
//! into a fresh provider. Subsequent calls reuse the worker.

use crate::curator::api_anthropic::AnthropicProvider;
use crate::curator::api_gemini::GeminiProvider;
use crate::curator::api_local::LocalProvider;
use crate::curator::cost::estimate_cents_for_photo_count;
use crate::curator::provider::CuratorProvider;
use crate::curator::types::ShootSummary;
use crate::curator::{
    keyring_account_for, spawn_worker, CuratorJob, CuratorStatus, ProgressCounters, KEYRING_SERVICE,
};
use crate::db::schema::{CuratorAgreementStats, Settings};
use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

// ---- Key management ----

/// Generic key setter. `provider` is `"anthropic" | "gemini" | "local"`.
#[tauri::command]
pub fn set_curator_api_key(provider: String, api_key: String) -> Result<(), String> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err("API key is empty".into());
    }
    let account = keyring_account_for(&provider);
    let entry = keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
    entry.set_password(trimmed).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_curator_api_key(provider: String) -> Result<(), String> {
    let account = keyring_account_for(&provider);
    let entry = keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    pub configured: bool,
    /// Last 4 chars of the key for display (`••••••••cdef`). Empty when
    /// no key is configured.
    pub suffix: String,
}

#[tauri::command]
pub fn get_curator_api_key_status(provider: String) -> Result<ApiKeyStatus, String> {
    let account = keyring_account_for(&provider);
    let entry = keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(k) => {
            let suffix = if k.len() >= 4 {
                k[k.len().saturating_sub(4)..].to_string()
            } else {
                String::new()
            };
            Ok(ApiKeyStatus { configured: true, suffix })
        }
        Err(keyring::Error::NoEntry) => Ok(ApiKeyStatus {
            configured: false,
            suffix: String::new(),
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Test the connection for the currently selected curator provider.
/// For local, no key is required.
#[tauri::command]
pub async fn test_curator_connection(
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let settings = read_settings(&state)?;
    let provider = build_provider_from_settings(&settings)?;
    provider.test_connection().await.map_err(|e| e.to_string())
}

// ---- Backwards-compatible Anthropic-named shims ----
//
// The existing frontend still calls these. They forward to the generic
// commands above. Remove once the UI in Phase 5 switches over fully.

#[tauri::command]
pub fn set_anthropic_api_key(api_key: String) -> Result<(), String> {
    set_curator_api_key("anthropic".to_string(), api_key)
}

#[tauri::command]
pub fn clear_anthropic_api_key() -> Result<(), String> {
    clear_curator_api_key("anthropic".to_string())
}

#[tauri::command]
pub fn get_anthropic_api_key_status() -> Result<ApiKeyStatus, String> {
    get_curator_api_key_status("anthropic".to_string())
}

#[tauri::command]
pub async fn test_anthropic_connection(
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    test_curator_connection(state).await
}

// ---- Run control ----

#[tauri::command]
pub fn start_curator_for_shoot(
    shoot_id: i64,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    // Drop any idle worker so its provider is rebuilt from current
    // settings. The worker caches the LocalProvider's model + base URL
    // at spawn time; without this, settings changes don't take effect
    // until app restart. We only drop when no run is active so we don't
    // interrupt in-flight Stage 2 work.
    {
        let mut s = state.lock().map_err(|e| e.to_string())?;
        if matches!(
            s.curator_status,
            CuratorStatus::Idle | CuratorStatus::Failed | CuratorStatus::Disabled
        ) {
            s.curator_worker = None;
        }
    }

    ensure_worker_spawned(&app, &state)?;

    let mut s = state.lock().map_err(|e| e.to_string())?;
    // Clone the sender so we can mutate other AppState fields without
    // tripping the borrow checker.
    let sender = s
        .curator_worker
        .as_ref()
        .ok_or("curator worker not running")?
        .sender
        .clone();
    s.curator_processed.store(0, Ordering::SeqCst);
    s.curator_failed.store(0, Ordering::SeqCst);
    s.curator_total.store(0, Ordering::SeqCst);
    s.curator_cost_cents.store(0, Ordering::SeqCst);
    s.curator_cancel.store(false, Ordering::SeqCst);
    s.curator_status = CuratorStatus::Running;
    s.curator_running_shoot_id = Some(shoot_id);
    sender
        .send(CuratorJob::Stage1 { shoot_id })
        .map_err(|e| format!("send Stage1: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn cancel_curator(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.curator_cancel.store(true, Ordering::SeqCst);
    s.curator_status = CuratorStatus::Idle;
    Ok(())
}

#[tauri::command]
pub fn resume_curator_for_shoot(
    shoot_id: i64,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    ensure_worker_spawned(&app, &state)?;
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    let summary_present = db
        .curator_summary_json(shoot_id)
        .map_err(|e| e.to_string())?
        .is_some();
    if !summary_present {
        // Treat resume on a never-started shoot as a fresh start.
        drop(s);
        return start_curator_for_shoot(shoot_id, app, state);
    }
    let pending = db
        .photos_needing_curator(shoot_id)
        .map_err(|e| e.to_string())?;
    if pending.is_empty() {
        return Ok(());
    }
    let groups = db
        .get_groups_for_shoot(shoot_id)
        .map_err(|e| e.to_string())?;
    let pending_set: std::collections::HashSet<i64> = pending.iter().copied().collect();
    let worker = s.curator_worker.as_ref().ok_or("curator worker not running")?;
    let mut grouped_pending: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut sent = 0usize;
    for grp in &groups {
        if grp.members.iter().any(|m| pending_set.contains(&m.photo_id)) {
            for m in &grp.members {
                grouped_pending.insert(m.photo_id);
            }
            worker
                .sender
                .send(CuratorJob::Stage2Cluster {
                    shoot_id,
                    group_id: grp.id,
                })
                .ok();
            sent += 1;
        }
    }
    let singletons: Vec<i64> = pending
        .iter()
        .copied()
        .filter(|id| !grouped_pending.contains(id))
        .collect();
    for chunk in singletons.chunks(6) {
        worker
            .sender
            .send(CuratorJob::Stage2Singletons {
                shoot_id,
                photo_ids: chunk.to_vec(),
            })
            .ok();
        sent += 1;
    }
    s.curator_total.store(sent, Ordering::SeqCst);
    s.curator_processed.store(0, Ordering::SeqCst);
    s.curator_cost_cents.store(0, Ordering::SeqCst);
    s.curator_cancel.store(false, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn clear_curator_for_shoot(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    db.clear_curator_for_shoot(shoot_id)
        .map_err(|e| e.to_string())
}

// ---- Read accessors ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CuratorRunStatus {
    pub status: CuratorStatus,
    pub running_shoot_id: Option<i64>,
    pub processed: usize,
    pub failed: usize,
    pub total: usize,
    pub cost_cents: usize,
}

#[tauri::command]
pub fn get_curator_status(state: State<'_, Mutex<AppState>>) -> Result<CuratorRunStatus, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(CuratorRunStatus {
        status: s.curator_status,
        running_shoot_id: s.curator_running_shoot_id,
        processed: s.curator_processed.load(Ordering::SeqCst),
        failed: s.curator_failed.load(Ordering::SeqCst),
        total: s.curator_total.load(Ordering::SeqCst),
        cost_cents: s.curator_cost_cents.load(Ordering::SeqCst),
    })
}

#[tauri::command]
pub fn get_curator_judgment_for_photo(
    photo_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<crate::db::schema::CuratorJudgmentRow>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    db.curator_judgment_for_photo(photo_id)
        .map_err(|e| e.to_string())
}

/// Bulk-load every curator judgment for a shoot. Used by the frontend
/// at `loadShoot` so Triage filtering and Select rank lookups can run
/// off a local map without per-photo IPC.
#[tauri::command]
pub fn get_curator_judgments_for_shoot(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<crate::db::schema::CuratorJudgmentRow>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    db.curator_judgments_for_shoot(shoot_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_curator_summary(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<ShootSummary>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    let json_opt = db
        .curator_summary_json(shoot_id)
        .map_err(|e| e.to_string())?;
    let Some(json) = json_opt else { return Ok(None) };
    serde_json::from_str(&json).map(Some).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_curator_agreement_stats(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<CuratorAgreementStats, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    db.curator_agreement_stats(shoot_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn estimate_curator_cost_cents(photo_count: i64) -> u32 {
    estimate_cents_for_photo_count(photo_count)
}

/// Mark a Claude suggestion as deliberately accepted. Returns the
/// `suggested_flag` string ("pick" | "reject" | "keep") so the
/// frontend can decide whether (and which) flag to write through the
/// normal `set_flag` flow. This separation keeps the undo log
/// uniform — the only place that ever writes `photos.flag` is the
/// existing `set_flag` command.
#[tauri::command]
pub fn accept_curator_suggestion(
    photo_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<String, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    let judgment = db
        .curator_judgment_for_photo(photo_id)
        .map_err(|e| e.to_string())?
        .ok_or("no curator judgment for photo")?;
    db.set_curator_user_action(photo_id, "accepted")
        .map_err(|e| e.to_string())?;
    Ok(judgment.suggested_flag)
}

/// Record that a manual P/X disagreed with the suggestion. Called by
/// the frontend right after the normal `set_flag` command when the
/// user's choice differs from `suggested_flag`. Returns silently when
/// there's no judgment row (no-op for photos Claude hasn't analyzed).
#[tauri::command]
pub fn record_curator_override(
    photo_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    let exists = db
        .curator_judgment_for_photo(photo_id)
        .map_err(|e| e.to_string())?
        .is_some();
    if !exists {
        return Ok(());
    }
    db.set_curator_user_action(photo_id, "overridden")
        .map_err(|e| e.to_string())
}

// ---- Triage stage ----

/// Bulk-load every triage-stage judgment for a shoot. The frontend reads
/// this at `loadShoot` so the Triage "AI rejects" filter has a local map.
#[tauri::command]
pub fn get_triage_judgments_for_shoot(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<crate::db::schema::TriageJudgmentRow>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    db.triage_judgments_for_shoot(shoot_id)
        .map_err(|e| e.to_string())
}

/// Apply pending triage-stage rejects: write `flag = 'reject'` for every
/// photo the triage stage flagged that the user hasn't already triaged by
/// hand. Idempotent — each judgment is marked `applied` once acted on, so
/// repeated calls (one per `curator:triage_done` event) only ever touch
/// new rows. Returns the photo IDs actually flagged so the frontend can
/// fold them into one batch undo entry.
#[tauri::command]
pub fn apply_triage_rejects(
    shoot_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<i64>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    let pending = db
        .pending_triage_rejects(shoot_id)
        .map_err(|e| e.to_string())?;
    let mut flagged = Vec::new();
    for pid in pending {
        // Only auto-reject photos still unreviewed — never overwrite a
        // decision the user already made by hand.
        if let Ok(photo) = db.get_photo_by_id(pid) {
            if photo.flag == "unreviewed" {
                db.set_flag(pid, "reject").map_err(|e| e.to_string())?;
                let _ = db.append_undo(
                    shoot_id,
                    &s.session_id,
                    pid,
                    "flag",
                    "unreviewed",
                    "reject",
                );
                flagged.push(pid);
            }
        }
        db.mark_triage_judgment_applied(pid)
            .map_err(|e| e.to_string())?;
    }
    Ok(flagged)
}

// ---- Internal helpers ----

fn read_settings(state: &State<'_, Mutex<AppState>>) -> Result<Settings, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let db = s.db.as_ref().ok_or("db not open")?;
    db.get_settings().map_err(|e| e.to_string())
}

fn read_key(provider: &str) -> Result<String, String> {
    let account = keyring_account_for(provider);
    let entry =
        keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => format!("No {} API key configured", provider),
        other => other.to_string(),
    })
}

/// Build the right `CuratorProvider` impl for the user's settings. Phase
/// 1 only knows how to construct `AnthropicProvider`; Phases 3+ extend
/// this with Gemini + Local without touching callers.
fn build_provider_from_settings(
    settings: &Settings,
) -> Result<Arc<dyn CuratorProvider>, String> {
    match settings.curator_provider.as_str() {
        "anthropic" | "" => {
            let key = read_key("anthropic")?;
            let model = if settings.curator_model_anthropic.is_empty() {
                settings.curator_model.clone()
            } else {
                settings.curator_model_anthropic.clone()
            };
            let p = AnthropicProvider::new(key, model).map_err(|e| e.to_string())?;
            Ok(Arc::new(p) as Arc<dyn CuratorProvider>)
        }
        "gemini" => {
            let key = read_key("gemini")?;
            let model = if settings.curator_model_gemini.is_empty() {
                crate::curator::default_model_for("gemini").to_string()
            } else {
                settings.curator_model_gemini.clone()
            };
            let p = GeminiProvider::new(key, model).map_err(|e| e.to_string())?;
            Ok(Arc::new(p) as Arc<dyn CuratorProvider>)
        }
        "local" => {
            // Optional bearer token. Most local servers don't need one,
            // so we silently ignore a missing keychain entry.
            let api_key = read_key("local").ok();
            let model = settings.curator_model_local.clone();
            let base_url = settings.curator_local_base_url.clone();
            let p =
                LocalProvider::new(base_url, model, api_key).map_err(|e| e.to_string())?;
            Ok(Arc::new(p) as Arc<dyn CuratorProvider>)
        }
        other => Err(format!("unknown curator provider: {}", other)),
    }
}

/// Lazy-spawn the curator worker the first time it's needed. Idempotent.
/// `pub(crate)` so the import pipeline can spawn the shared worker for
/// the on-import triage stage.
pub(crate) fn ensure_worker_spawned(
    app: &AppHandle,
    state: &State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    {
        let s = state.lock().map_err(|e| e.to_string())?;
        if s.curator_worker.is_some() {
            return Ok(());
        }
    }
    let settings = read_settings(state)?;
    let provider = build_provider_from_settings(&settings)?;
    let cost_cap = settings.curator_max_cost_per_shoot_cents.max(0) as u32;
    let db_path = crate::db::schema::global_db_path();

    let progress = {
        let s = state.lock().map_err(|e| e.to_string())?;
        ProgressCounters {
            processed: s.curator_processed.clone(),
            failed: s.curator_failed.clone(),
            total: s.curator_total.clone(),
            cost_cents: s.curator_cost_cents.clone(),
        }
    };
    let cancel = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.curator_cancel.clone()
    };
    let handle = spawn_worker(db_path, provider, cancel, progress, cost_cap, app.clone());
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.curator_worker = Some(handle);
    s.curator_status = CuratorStatus::Idle;
    Ok(())
}

