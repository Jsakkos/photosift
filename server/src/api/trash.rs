//! `POST /api/shoots/{id}/trash`: the explicit batch approval for deletes.

use axum::{
    extract::{Path, State},
    Json,
};

use serde::Deserialize;

use super::{ApiError, AppState};
use crate::{
    db::review,
    enrich::moments,
    trash::{self, TrashReport},
};

#[derive(Deserialize)]
pub struct TrashRequest {
    /// The photos the confirm dialog listed; only these can be trashed.
    pub photo_ids: Vec<i64>,
}

pub async fn trash_shoot(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<TrashRequest>,
) -> Result<Json<TrashReport>, ApiError> {
    tokio::task::spawn_blocking(move || {
        if review::get_shoot(&state.db, id)?.is_none() {
            return Ok(None);
        }
        let report = trash::trash_approved_deletes(&state.db, id, &body.photo_ids)?;
        if report.trashed > 0 {
            // Re-number moments and bursts without the trashed frames.
            if let Err(e) = moments::assign(&state.db, id, &state.settings) {
                eprintln!("photosift: regrouping shoot {id} after trash: {e:#}");
            }
        }
        Ok::<_, anyhow::Error>(Some(report))
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(|e| ApiError::internal(format!("{e:#}")))?
    .map(Json)
    .ok_or_else(|| ApiError::not_found(format!("no shoot {id}")))
}
