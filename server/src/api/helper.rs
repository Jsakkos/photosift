//! The helper's routes: run it on a shoot now, and chat with it.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ApiError, AppState};
use crate::{
    db::helper::{self, ChatMessageRow, DecisionRow, RunKind, TasteAuthor, TasteRow, TASTE_MAX},
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
        Err(e) => Err(enqueue_error(e)),
    }
}

fn enqueue_error(e: EnqueueError) -> ApiError {
    match e {
        EnqueueError::Unavailable(_) => ApiError::unavailable(e.to_string()),
        EnqueueError::Conflict(_) | EnqueueError::ChatBusy(_) => ApiError::conflict(e.to_string()),
        EnqueueError::NotFound(_) => ApiError::not_found(e.to_string()),
        EnqueueError::Invalid(_) => ApiError::bad_request(e.to_string()),
        EnqueueError::Other(e) => ApiError::internal(format!("{e:#}")),
    }
}

#[derive(Deserialize)]
pub struct ChatBody {
    text: String,
}

/// 202 once the message is stored and the helper is answering (the answer
/// arrives as `chat_message` events); 409 while the previous turn for the
/// shoot is still running; 503 with the reason when the helper cannot run.
pub async fn chat(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ChatBody>,
) -> Result<StatusCode, ApiError> {
    let helper = state.helper.clone();
    tokio::task::spawn_blocking(move || {
        if helper.unavailable_reason().is_some() {
            if let Availability::Unavailable { reason } = helper.refresh_availability() {
                return Err(EnqueueError::Unavailable(reason));
            }
        }
        helper.chat(id, &body.text)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(enqueue_error)?;
    Ok(StatusCode::ACCEPTED)
}

/// The shoot's conversation with the helper, oldest first.
pub async fn chat_history(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<ChatMessageRow>>, ApiError> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || helper::chat_messages(&db, id))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map(Json)
        .map_err(|e| ApiError::internal(format!("{e:#}")))
}

pub async fn get_taste(State(state): State<AppState>) -> Result<Json<TasteRow>, ApiError> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || helper::get_taste_row(&db))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map(Json)
        .map_err(|e| ApiError::internal(format!("{e:#}")))
}

#[derive(Deserialize)]
pub struct TasteBody {
    text: String,
}

/// The user's own save: marks the profile as written by the user.
pub async fn put_taste(
    State(state): State<AppState>,
    Json(body): Json<TasteBody>,
) -> Result<Json<TasteRow>, ApiError> {
    let n = body.text.chars().count();
    if n > TASTE_MAX {
        return Err(ApiError::bad_request(format!(
            "the taste profile is {n} characters; the limit is {TASTE_MAX}"
        )));
    }
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        helper::set_taste(&db, &body.text, TasteAuthor::User)?;
        helper::get_taste_row(&db)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map(Json)
    .map_err(|e| ApiError::internal(format!("{e:#}")))
}

#[derive(Deserialize)]
pub struct DecisionsQuery {
    limit: Option<usize>,
}

/// The newest overrides, newest first; `limit` defaults to 50 (at most 200).
pub async fn taste_decisions(
    State(state): State<AppState>,
    Query(q): Query<DecisionsQuery>,
) -> Result<Json<Vec<DecisionRow>>, ApiError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || helper::recent_overrides(&db, limit))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map(Json)
        .map_err(|e| ApiError::internal(format!("{e:#}")))
}
