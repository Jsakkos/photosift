//! Shoot list and the review screen's data.

use axum::{
    extract::{Path, State},
    Json,
};

use super::{ApiError, AppState};
use crate::db::review::{self, ShootReview, ShootSummary};

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<ShootSummary>>, ApiError> {
    let shoots = tokio::task::spawn_blocking(move || review::list_shoots(&state.db))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::internal(format!("{e:#}")))?;
    Ok(Json(shoots))
}

pub async fn review(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ShootReview>, ApiError> {
    tokio::task::spawn_blocking(move || review::shoot_review(&state.db, id))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::internal(format!("{e:#}")))?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("no shoot {id}")))
}
