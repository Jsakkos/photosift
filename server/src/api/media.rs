//! `/media/preview/{id}` and `/media/thumb/{id}`: the cached JPEGs.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};

use super::{ApiError, AppState};
use crate::ingest::scan::ItemKind;
use crate::media::{embedded, exif, render};

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

/// Thumbnails of what is still on the card. They are not library photos (no
/// row, no id), so they are keyed by scan and item and cached under
/// `cache/scan/<scan id>/`, which startup clears.
pub async fn scan_thumb(
    State(state): State<AppState>,
    Path((scan_id, idx)): Path<(String, usize)>,
) -> Result<Response, ApiError> {
    // Only ids the scan cache knows reach the filesystem path below.
    let scan = state
        .scans
        .get(&scan_id)
        .ok_or_else(|| ApiError::not_found("unknown scan; scan the card again"))?;
    let item = scan
        .items
        .get(idx)
        .filter(|i| i.kind == ItemKind::Photo)
        .ok_or_else(|| ApiError::not_found(format!("no picture for item {idx}")))?;
    let cached = state
        .data_root
        .join("cache")
        .join("scan")
        .join(&scan_id)
        .join(format!("{idx}.jpg"));
    // The camera JPEG is much smaller to read off a slow card than the RAW.
    let source = item
        .sibling_jpeg
        .clone()
        .unwrap_or_else(|| item.primary.clone());
    let bytes = tokio::task::spawn_blocking(move || scan_thumb_bytes(&cached, source))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::internal(format!("thumbnail for item {idx}: {e:#}")))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, IMMUTABLE),
        ],
        bytes,
    )
        .into_response())
}

fn scan_thumb_bytes(cached: &std::path::Path, source: PathBuf) -> anyhow::Result<Vec<u8>> {
    use anyhow::Context;
    if let Ok(bytes) = std::fs::read(cached) {
        return Ok(bytes);
    }
    let raw = std::fs::read(&source).with_context(|| format!("reading {}", source.display()))?;
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let jpeg = embedded::largest_jpeg(&raw, ext)?;
    let orientation = exif::read(&raw, &source).orientation;
    let thumb = render::thumbnail_jpeg(jpeg, orientation)?;
    // Best effort: a failed cache write only costs a re-render next time. The
    // bytes are served with `immutable`, so a half-written cache file must
    // never be readable: write under a name no reader opens, then rename.
    if let Some(dir) = cached.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    static WRITER: AtomicU64 = AtomicU64::new(0);
    let tmp = cached.with_extension(format!(
        "jpg.{}.tmp",
        WRITER.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::write(&tmp, &thumb)
        .and_then(|()| std::fs::rename(&tmp, cached))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(thumb)
}
