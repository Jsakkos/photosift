//! `POST /api/import`: start a background import of a previously scanned card;
//! `POST /api/import/in-place`: open a library folder as a shoot, uncopied.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::scan::is_local_drive_path;
use super::{ApiError, AppState};
use crate::events::{JobState, ServerEvent};
use crate::ingest::in_place;
use crate::ingest::job::{self, EventSpec, ImportMode, ImportRequest, JobCtx};
use crate::ingest::scan::{new_id, CardScan};

#[derive(Deserialize)]
pub struct ImportBody {
    pub scan_id: String,
    pub events: Vec<EventSpec>,
    #[serde(default)]
    pub camera_offsets: HashMap<String, i64>,
}

/// Clears the single-job flag however the job thread ends.
struct BusyGuard(AppState);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.import_busy.store(false, Ordering::SeqCst);
    }
}

/// State of one job, from the in-memory log of the last few: lets a page that
/// missed its WebSocket events catch up. 404 once the job has been forgotten
/// (or after a restart).
pub async fn job_state(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobState>, ApiError> {
    state
        .events
        .jobs()
        .get(&job_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("unknown job"))
}

/// The running job, if any, so a page can explain a 409 and resync its
/// "busy" state.
pub async fn current(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "job_id": state.events.jobs().running() }))
}

pub async fn start(
    State(state): State<AppState>,
    Json(body): Json<ImportBody>,
) -> Result<Json<Value>, ApiError> {
    let scan = state
        .scans
        .get(&body.scan_id)
        .ok_or_else(|| ApiError::not_found("unknown scan_id; scan the card again"))?;
    let request = ImportRequest {
        events: body.events,
        camera_offsets: body.camera_offsets,
    };
    job::validate(&scan, &request).map_err(ApiError::bad_request)?;
    let guard = claim_import(&state)?;
    let job_id = spawn_job(&state, guard, scan, request, ImportMode::Copy)?;
    Ok(Json(json!({ "job_id": job_id })))
}

#[derive(Deserialize)]
pub struct InPlaceBody {
    pub folder: PathBuf,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InPlaceReply {
    /// The folder is a shoot already (opened before, or imported into).
    Existing { shoot_id: i64 },
    /// Indexing runs as an import job; its `import_done` names the shoot.
    Indexing { job_id: String, photos: usize },
}

/// `POST /api/import/in-place`: opens a folder of the library as one shoot
/// without copying it (e.g. a shoot Lightroom imported). Outcomes then go to
/// sidecars beside those files, as for any shoot.
pub async fn in_place(
    State(state): State<AppState>,
    Json(body): Json<InPlaceBody>,
) -> Result<Json<InPlaceReply>, ApiError> {
    if !is_local_drive_path(&body.folder) {
        return Err(ApiError::bad_request(
            "only absolute paths on a local drive letter can be opened (no network, device or relative paths)",
        ));
    }
    let folder = in_place::resolve_in_library(&body.folder, &state.settings.library_root)
        .map_err(ApiError::bad_request)?;
    let existing = |state: &AppState| {
        in_place::shoot_for_folder(&state.db, &folder)
            .map_err(|e| ApiError::internal(format!("{e:#}")))
    };
    if let Some(shoot_id) = existing(&state)? {
        return Ok(Json(InPlaceReply::Existing { shoot_id }));
    }
    let guard = claim_import(&state)?;
    // An open of the same folder may have finished while this one waited.
    if let Some(shoot_id) = existing(&state)? {
        return Ok(Json(InPlaceReply::Existing { shoot_id }));
    }
    let (db, settings) = (state.db.clone(), state.settings.clone());
    let scan_folder = folder.clone();
    let (scan, request) =
        tokio::task::spawn_blocking(move || in_place::plan(&scan_folder, &db, &settings))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    let photos = request.events.iter().map(|e| e.item_idxs.len()).sum();
    let job_id = spawn_job(
        &state,
        guard,
        Arc::new(scan),
        request,
        ImportMode::IndexInPlace,
    )?;
    Ok(Json(InPlaceReply::Indexing { job_id, photos }))
}

/// One import at a time: jobs share destination folders and the cache's
/// scratch dir, and each job sweeps stale files there.
fn claim_import(state: &AppState) -> Result<BusyGuard, ApiError> {
    state
        .import_busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| BusyGuard(state.clone()))
        .map_err(|_| ApiError::conflict("an import is already running"))
}

/// Runs the job on its own thread, which releases `guard` when it ends.
fn spawn_job(
    state: &AppState,
    guard: BusyGuard,
    scan: Arc<CardScan>,
    request: ImportRequest,
    mode: ImportMode,
) -> Result<String, ApiError> {
    let job_id = new_id();
    state.events.jobs().begin(&job_id);
    let ctx = JobCtx {
        db: state.db.clone(),
        events: state.events.clone(),
        enrich: state.enrich.clone(),
        library_root: state.settings.library_root.clone(),
        cache_root: state.data_root.join("cache"),
    };
    let thread_job_id = job_id.clone();
    std::thread::Builder::new()
        .name("import".into())
        .spawn(move || {
            let _guard = guard;
            // The outcome is reported as an event.
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                job::run_with_mode(&ctx, &thread_job_id, &scan, &request, mode)
            }));
            if outcome.is_err() {
                ctx.events.send(ServerEvent::ImportFailed {
                    job_id: thread_job_id,
                    error: "the import job crashed; see the server log".into(),
                });
            }
        })
        .map_err(|e| ApiError::internal(format!("starting import thread: {e}")))?;
    Ok(job_id)
}
