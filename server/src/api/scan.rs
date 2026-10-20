//! `POST /api/scan`: walk a card or folder and propose events.

use std::path::{Component, Path, PathBuf, Prefix};
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
    if !is_local_drive_path(&source) {
        return Err(ApiError::bad_request(
            "only absolute paths on a local drive letter can be scanned (no network, device or relative paths)",
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

/// Allowlist rather than a blocklist of UNC spellings: Windows accepts many
/// (`\/h\s`, `\??\UNC\h\s`, `\\.\pipe\x`), and reading any network path makes
/// the machine authenticate to that host, leaking an NTLM hash to whoever a
/// hostile page named. Only `C:\...` and `\\?\C:\...` pass.
fn is_local_drive_path(path: &Path) -> bool {
    let mut parts = path.components();
    matches!(
        parts.next(),
        Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    ) && matches!(parts.next(), Some(Component::RootDir))
}
