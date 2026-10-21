//! `POST /api/import`: start a background import of a previously scanned card.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ApiError, AppState};
use crate::events::{JobState, ServerEvent};
use crate::ingest::job::{self, EventSpec, ImportRequest, JobCtx};
use crate::ingest::scan::new_id;

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

    // One import at a time: jobs share destination folders, and each job
    // sweeps stale .partial files there.
    if state
        .import_busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(ApiError::conflict("an import is already running"));
    }
    let guard = BusyGuard(state.clone());
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
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                job::run(&ctx, &thread_job_id, &scan, &request)
            }));
            if outcome.is_err() {
                ctx.events.send(ServerEvent::ImportFailed {
                    job_id: thread_job_id,
                    error: "the import job crashed; see the server log".into(),
                });
            }
        })
        .map_err(|e| ApiError::internal(format!("starting import thread: {e}")))?;
    Ok(Json(json!({ "job_id": job_id })))
}
