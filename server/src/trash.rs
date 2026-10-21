//! Approved delete: sends a photo's files to the Windows Recycle Bin.
//!
//! Only `trash::delete_all` is used; nothing here can delete permanently.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

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
    /// Approved ids that were not pending deletes of this shoot any more
    /// (unknown, other shoot, outcome changed since, already trashed).
    pub skipped: usize,
}

/// Every file the photo owns: its RAW, JPEG and video, plus exactly one
/// sidecar, the one `api/photos.rs` writes (`<stem>.xmp` of the RAW, else of
/// the JPEG). A JPEG attached to a RAW under a different stem has no sidecar
/// of its own, and `<jpeg stem>.xmp` may belong to another photo.
fn owned_files(photo: &PhotoRow) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let sidecar_of = photo.raw_path.as_deref().or(photo.jpeg_path.as_deref());
    paths.extend(sidecar_of.map(|p| xmp::sidecar_path(Path::new(p))));
    paths.extend(photo.raw_path.as_deref().map(PathBuf::from));
    paths.extend(photo.jpeg_path.as_deref().map(PathBuf::from));
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

/// Trashes the pending deletes of the shoot that the user approved, i.e. the
/// ones in `approved` (the exact set the confirm dialog listed). A photo marked
/// delete after that dialog opened is not in `approved` and stays. See
/// `trash_photos`. With `read_only` (`Settings::read_only`, the eval
/// harness) it refuses outright and touches no file.
pub fn trash_approved_deletes(
    db: &DbPool,
    shoot_id: i64,
    approved: &[i64],
    read_only: bool,
) -> anyhow::Result<TrashReport> {
    if read_only {
        anyhow::bail!("read-only: nothing is sent to the Recycle Bin");
    }
    let wanted: HashSet<i64> = approved.iter().copied().collect();
    let snapshot: Vec<PhotoRow> = photos::pending_deletes(db, shoot_id)?
        .into_iter()
        .filter(|p| wanted.contains(&p.id))
        .collect();
    let skipped = wanted.len() - snapshot.len();
    let mut report = trash_photos(db, shoot_id, snapshot);
    report.skipped += skipped;
    Ok(report)
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
