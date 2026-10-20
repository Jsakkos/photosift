//! `POST /api/photos/{id}/outcome`: set a photo's outcome and mirror it to XMP.

use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::{ApiError, AppState};
use crate::{
    db::photos::{self, PhotoRow},
    outcome::{xmp_fields, Outcome},
    xmp,
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
        let photo = photos::set_outcome(&db, id, body.outcome, body.blooper)?;
        let xmp_error = sidecar_target(&photo).and_then(|image| {
            xmp::apply(&image, &xmp_fields(photo.outcome, photo.blooper))
                .err()
                .map(|e| e.to_string())
        });
        Ok::<_, anyhow::Error>(OutcomeResponse { photo, xmp_error })
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    match res {
        Ok(r) => Ok(Json(r)),
        Err(e)
            if matches!(
                e.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::QueryReturnedNoRows)
            ) =>
        {
            Err(ApiError::not_found(format!("no photo {id}")))
        }
        Err(e) => Err(ApiError::internal(format!("{e:#}"))),
    }
}

/// The file whose `<stem>.xmp` carries the outcome: the RAW, else the JPEG.
/// Videos get no sidecar.
fn sidecar_target(photo: &PhotoRow) -> Option<PathBuf> {
    photo
        .raw_path
        .as_deref()
        .or(photo.jpeg_path.as_deref())
        .map(PathBuf::from)
}
