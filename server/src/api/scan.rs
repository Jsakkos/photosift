//! `POST /api/scan`: walk a card or folder and propose events.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;

use super::{ApiError, AppState};
use crate::ingest::scan::{self, CardScan};

#[derive(Deserialize)]
pub struct ScanBody {
    pub source: PathBuf,
}

pub async fn scan(
    State(state): State<AppState>,
    Json(body): Json<ScanBody>,
) -> Result<Json<Arc<CardScan>>, ApiError> {
    let (db, settings) = (state.db.clone(), state.settings.clone());
    let source = body.source;
    // The walk reads EXIF off a slow card; keep it off the async runtime.
    let card = tokio::task::spawn_blocking(move || scan::scan(&source, &db, &settings))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    let card = Arc::new(card);
    state.scans.insert(card.clone());
    Ok(Json(card))
}
