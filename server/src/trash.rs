//! Approved delete: sends a photo's files to the Windows Recycle Bin.
//!
//! Only `trash::delete_all` is used; nothing here can delete permanently.

use std::path::PathBuf;

use serde::Serialize;

use crate::{
    db::{
        photos::{self, PhotoRow},
        DbPool,
    },
    xmp,
};

#[derive(Debug, Clone, Serialize)]
pub struct TrashFailure {
    pub photo_id: i64,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TrashReport {
    pub trashed: usize,
    pub failed: Vec<TrashFailure>,
}

/// The photo's files that still exist: RAW, sibling JPEG, video, and the
/// `.xmp` sidecar beside the RAW and beside the JPEG. Missing files are
/// skipped, so a retry after a partial failure only handles what is left.
fn files_of(photo: &PhotoRow) -> Vec<PathBuf> {
    let images = [photo.raw_path.as_deref(), photo.jpeg_path.as_deref()];
    let mut paths: Vec<PathBuf> = Vec::new();
    for image in images.into_iter().flatten().map(PathBuf::from) {
        paths.push(xmp::sidecar_path(&image));
        paths.push(image);
    }
    paths.extend(photo.video_path.as_deref().map(PathBuf::from));
    paths.sort();
    paths.dedup();
    paths.retain(|p| p.exists());
    paths
}

/// Trashes every `delete` photo of the shoot, one photo at a time so a locked
/// file fails only its own photo (reported, left in the review, retryable).
/// Successful photos get `trashed_at`; their `files` rows are kept on purpose
/// so a later import from the card still recognises and skips the junk.
pub fn trash_approved_deletes(db: &DbPool, shoot_id: i64) -> anyhow::Result<TrashReport> {
    let mut report = TrashReport::default();
    for photo in photos::pending_deletes(db, shoot_id)? {
        let files = files_of(&photo);
        let result = if files.is_empty() {
            Ok(())
        } else {
            ::trash::delete_all(&files).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => {
                photos::mark_trashed(db, photo.id)?;
                report.trashed += 1;
            }
            Err(error) => report.failed.push(TrashFailure {
                photo_id: photo.id,
                error,
            }),
        }
    }
    Ok(report)
}
