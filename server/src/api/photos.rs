//! `POST /api/photos/{id}/outcome`: set a photo's outcome and mirror it to XMP.
//! `POST /api/photos/{id}/decision-note`: why the user overrode the helper.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use super::{ApiError, AppState};
use crate::{
    db::photos::PhotoRow,
    helper::learning,
    outcome::{self, ApplyError, Outcome},
    plog,
};

/// Longest note the user may attach to a decision.
const NOTE_MAX: usize = 500;

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
    let read_only = state.settings.read_only;
    let res = tokio::task::spawn_blocking(move || {
        let res = outcome::set_and_mirror(&db, id, body.outcome, body.blooper, read_only);
        if let Ok((photo, _)) = &res {
            if let Err(e) = learning::record_decision(&db, id, photo.outcome) {
                plog!(
                    "helper",
                    "recording the decision for photo {id} failed: {e:#}"
                );
            }
        }
        res
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

#[derive(Deserialize)]
pub struct DecisionNoteBody {
    pub criterion: String,
    pub note: Option<String>,
}

pub async fn decision_note(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<DecisionNoteBody>,
) -> Result<StatusCode, ApiError> {
    if !learning::CRITERIA.contains(&body.criterion.as_str()) {
        return Err(ApiError::bad_request(format!(
            "criterion must be one of {}",
            learning::CRITERIA.join(", ")
        )));
    }
    let note = body
        .note
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_MAX) {
        return Err(ApiError::bad_request(format!(
            "note must be at most {NOTE_MAX} characters"
        )));
    }
    let db = state.db.clone();
    let found = tokio::task::spawn_blocking(move || {
        learning::set_criterion(&db, id, &body.criterion, note)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(|e| ApiError::internal(format!("{e:#}")))?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "photo {id} has no decision on its current proposal"
        )))
    }
}
