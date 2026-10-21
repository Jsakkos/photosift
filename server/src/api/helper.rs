//! `POST /api/shoots/{id}/helper/run`: run the helper on a shoot now.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use super::{ApiError, AppState};
use crate::{
    db::helper::RunKind,
    helper::{transport::Availability, EnqueueError},
};

/// 202 `{run_id}`; 409 while a run is queued or running for the shoot; 503
/// with the reason when the helper cannot run. An unavailable helper is
/// checked again first, so signing in and pressing the button again works.
pub async fn run(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let helper = state.helper.clone();
    let res = tokio::task::spawn_blocking(move || {
        if helper.unavailable_reason().is_some() {
            if let Availability::Unavailable { reason } = helper.refresh_availability() {
                return Err(EnqueueError::Unavailable(reason));
            }
        }
        helper.enqueue_kind(id, RunKind::Background)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    match res {
        Ok(run_id) => Ok((StatusCode::ACCEPTED, Json(json!({ "run_id": run_id })))),
        Err(e @ EnqueueError::Unavailable(_)) => Err(ApiError::unavailable(e.to_string())),
        Err(e @ EnqueueError::Conflict(_)) => Err(ApiError::conflict(e.to_string())),
        Err(e @ EnqueueError::NotFound(_)) => Err(ApiError::not_found(e.to_string())),
        Err(EnqueueError::Other(e)) => Err(ApiError::internal(format!("{e:#}"))),
    }
}
