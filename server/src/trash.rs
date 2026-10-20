//! Approved delete: sends a photo's files to the Windows Recycle Bin.
//!
//! Only `trash::delete_all` is used; nothing here can delete permanently.

use std::path::{Path, PathBuf};

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

/// Every file the photo owns: the RAW and the JPEG with the `.xmp` sidecar
/// beside each, and the video.
fn owned_files(photo: &PhotoRow) -> Vec<PathBuf> {
    let images = [photo.raw_path.as_deref(), photo.jpeg_path.as_deref()];
    let mut paths: Vec<PathBuf> = Vec::new();
    for image in images.into_iter().flatten().map(PathBuf::from) {
        paths.push(xmp::sidecar_path(&image));
        paths.push(image);
    }
    paths.extend(photo.video_path.as_deref().map(PathBuf::from));
    paths.sort();
    paths.dedup();
    paths
}

/// Whether `path` exists; an I/O error counts as "not there" so nothing is
/// ever concluded from a lookup that failed.
fn present(path: &Path) -> bool {
    path.try_exists().unwrap_or(false)
}

/// Trashes every `delete` photo of the shoot. See `trash_photos`.
pub fn trash_approved_deletes(db: &DbPool, shoot_id: i64) -> anyhow::Result<TrashReport> {
    let snapshot = photos::pending_deletes(db, shoot_id)?;
    Ok(trash_photos(db, shoot_id, snapshot))
}

/// Trashes the snapshot one photo at a time, so a locked file fails only its
/// own photo (reported, left in the review, retryable).
///
/// The snapshot can go stale while the batch runs (the user changes an
/// outcome, or a second request overlaps), so each photo is re-read right
/// before it is touched and skipped unless it is still a pending delete in
/// this shoot; `mark_trashed` re-checks the same predicate and only photos it
/// actually updated are counted.
///
/// Files already gone are skipped, so a retry after a partial failure only
/// handles what is left. A photo none of whose files exist is only marked
/// trashed when its folder exists: with the folder missing (drive offline) it
/// is reported as failed, since "gone" cannot be told from "unreachable".
///
/// Successful photos get `trashed_at`; their `files` rows are kept on purpose
/// so a later import from the card still recognises and skips the junk.
pub fn trash_photos(db: &DbPool, shoot_id: i64, snapshot: Vec<PhotoRow>) -> TrashReport {
    let mut report = TrashReport::default();
    for listed in snapshot {
        let fail = |report: &mut TrashReport, error: String| {
            report.failed.push(TrashFailure {
                photo_id: listed.id,
                error,
            })
        };
        let photo = match photos::pending_delete(db, shoot_id, listed.id) {
            Ok(Some(photo)) => photo,
            Ok(None) => continue,
            Err(e) => {
                fail(&mut report, format!("{e:#}"));
                continue;
            }
        };
        let owned = owned_files(&photo);
        let files: Vec<PathBuf> = owned.iter().filter(|p| present(p)).cloned().collect();
        if files.is_empty() {
            let folder_ok = owned.iter().filter_map(|p| p.parent()).any(present);
            if !owned.is_empty() && !folder_ok {
                fail(&mut report, "library folder not available".into());
                continue;
            }
        } else if let Err(e) = ::trash::delete_all(&files) {
            fail(&mut report, e.to_string());
            continue;
        }
        match photos::mark_trashed(db, shoot_id, photo.id) {
            Ok(true) => report.trashed += 1,
            // Changed or trashed by someone else since the re-read.
            Ok(false) => {}
            Err(e) => fail(&mut report, format!("trashed but not recorded: {e:#}")),
        }
    }
    report
}
