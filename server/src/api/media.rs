//! `/media/preview/{id}` and `/media/thumb/{id}`: the cached JPEGs.

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};

use super::{ApiError, AppState};

/// A preview is rendered once per photo id and never rewritten.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

pub async fn preview(state: State<AppState>, id: Path<i64>) -> Result<Response, ApiError> {
    serve(state, id, "preview").await
}

pub async fn thumb(state: State<AppState>, id: Path<i64>) -> Result<Response, ApiError> {
    serve(state, id, "thumb").await
}

async fn serve(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    kind: &'static str,
) -> Result<Response, ApiError> {
    let path = state
        .data_root
        .join("cache")
        .join(kind)
        .join(format!("{id}.jpg"));
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    match bytes {
        Ok(bytes) => Ok((
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, IMMUTABLE),
            ],
            bytes,
        )
            .into_response()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(ApiError::not_found(format!("no {kind} for photo {id}")))
        }
        Err(e) => Err(ApiError::internal(format!("reading {kind} {id}: {e}"))),
    }
}
