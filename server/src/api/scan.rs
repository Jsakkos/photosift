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
    // Reading a UNC path makes Windows authenticate to that host (leaking an
    // NTLM hash to whoever a hostile page names), so network sources are out.
    let text = source.to_string_lossy();
    if text.starts_with(r"\\") || text.starts_with("//") {
        return Err(ApiError::bad_request(
            "network (UNC) paths cannot be scanned; use a local or mapped drive",
        ));
    }
    // The walk reads EXIF off a slow card; keep it off the async runtime.
    let card = tokio::task::spawn_blocking(move || scan::scan(&source, &db, &settings))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    let card = Arc::new(card);
    state.scans.insert(card.clone());
    Ok(Json(card))
}
