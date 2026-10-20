//! `POST /api/scan`: walk a card or folder and propose events.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::Arc;

use axum::{
    extract::{Path as UrlPath, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::{ApiError, AppState};
use crate::ingest::events::{split_into_events, ProposedEvent};
use crate::ingest::naming::slugify;
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

/// The cached scan, so the import page can load it by the id in its URL. A
/// scan falls out of the cache after a few newer ones; the page then offers to
/// scan the card again.
pub async fn get_scan(
    State(state): State<AppState>,
    UrlPath(scan_id): UrlPath<String>,
) -> Result<Json<Arc<CardScan>>, ApiError> {
    state
        .scans
        .get(&scan_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("unknown scan; scan the card again"))
}

#[derive(Deserialize)]
pub struct RegroupBody {
    /// Seconds to add to each camera's clock, keyed by `ScanItem::camera`.
    #[serde(default)]
    pub camera_offsets: HashMap<String, i64>,
}

/// Re-proposes events for new camera offsets with the same rule the scan
/// used, so the page never carries its own copy of the split algorithm.
pub async fn regroup(
    State(state): State<AppState>,
    UrlPath(scan_id): UrlPath<String>,
    Json(body): Json<RegroupBody>,
) -> Result<Json<Vec<ProposedEvent>>, ApiError> {
    let scan = state
        .scans
        .get(&scan_id)
        .ok_or_else(|| ApiError::not_found("unknown scan; scan the card again"))?;
    Ok(Json(split_into_events(
        &scan.items,
        &body.camera_offsets,
        state.settings.event_gap_minutes,
        state.settings.everyday_max_photos,
    )))
}

#[derive(Deserialize)]
pub struct SlugBody {
    pub name: String,
}

#[derive(Serialize)]
pub struct SlugReply {
    pub slug: String,
}

/// The folder-name slug for an event name, so the page previews exactly what
/// the import will create.
pub async fn slug(Json(body): Json<SlugBody>) -> Json<SlugReply> {
    Json(SlugReply {
        slug: slugify(&body.name),
    })
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
