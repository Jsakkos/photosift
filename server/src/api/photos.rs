//! `POST /api/photos/{id}/outcome`: set a photo's outcome and mirror it to XMP.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::{ApiError, AppState};
use crate::{
    db::photos::PhotoRow,
    outcome::{self, ApplyError, Outcome},
};

#[derive(Deserialize)]
pub struct OutcomeBody {
    pub outcome: Outcome,
    /// `None` leaves the blooper tag as it is.
    pub blooper: Option<bool>,
}

#[derive(Serialize)]
pub struct OutcomeResponse {
    #[serde(flatten)]
    pub photo: PhotoRow,
    /// Set when the sidecar could not be written; the database change stands.
    pub xmp_error: Option<String>,
}

pub async fn set_outcome(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<OutcomeBody>,
) -> Result<Json<OutcomeResponse>, ApiError> {
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        outcome::set_and_mirror(&db, id, body.outcome, body.blooper)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    match res {
        Ok((photo, xmp_error)) => Ok(Json(OutcomeResponse { photo, xmp_error })),
        Err(ApplyError::NotFound(_)) => Err(ApiError::not_found(format!("no photo {id}"))),
        Err(e @ ApplyError::Trashed(_)) => Err(ApiError::conflict(e.to_string())),
        Err(ApplyError::Other(e)) => Err(ApiError::internal(format!("{e:#}"))),
    }
}
