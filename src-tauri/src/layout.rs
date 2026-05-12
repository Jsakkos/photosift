//! Auto-reorganize shoot folder layout based on cull state.
//!
//! After import every RAW sits in `{shoot}/RAW/`. As the user progresses
//! through triage → select → route we re-home files into bucket folders
//! so the filesystem reflects the cull while staying visually grouped.
//! Reject/select/edit buckets nest under `RAW/`; the export bucket is a
//! top-level `Export/` folder so finished/publishable artifacts sit
//! apart from the working RAW tree (and alongside whatever the user's
//! editor later drops there):
//!
//! ```text
//! DSLR/2026/2026-04_slug/
//!   RAW/              flag='unreviewed'
//!   RAW/rejects/      flag='reject'
//!   RAW/selects/      flag='pick', destination='unrouted'
//!   RAW/edit/         flag='pick', destination='edit'      (ready for Capture One / DxO / etc.)
//!   Export/           flag='pick', destination='export'    (ready for direct publish)
//! ```
//!
//! The bucket folder names above are the defaults — they're configurable
//! globally via `settings.folder_template` (see `crate::folder_template`),
//! so a sync may target e.g. `RAW/discards/` instead of `RAW/rejects/`.
//!
//! `sync_shoot_layout` is the sole mover. It reads truth from the DB,
//! moves each photo, writes a fresh XMP sidecar at the new location
//! reflecting the current `(rating, flag, destination)`, and updates
//! `photos.raw_path`. Idempotent — running twice in a row is a no-op on
//! the second call.
//!
//! Trigger gating lives in `trigger_is_eligible`: the frontend fires on
//! every view transition and the Rust side decides whether the stage is
//! "done" enough to actually move files.

use crate::db::schema::Database;
use crate::folder_template::Buckets;
use crate::metadata::xmp;
use rusqlite::Result as SqlResult;
use std::path::PathBuf;

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub moved: Vec<MoveRecord>,
    pub skipped_already_placed: usize,
    pub missing: Vec<String>,
    pub collisions: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRecord {
    pub photo_id: i64,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTrigger {
    /// Leaving triage for any other view.
    TriageComplete,
    /// Leaving select specifically for route.
    SelectComplete,
    /// Leaving route for any other view.
    RouteComplete,
}

/// Check whether the shoot is in a state where the trigger should fire.
/// Returns `Ok(true)` if `sync_shoot_layout` should be invoked.
pub fn trigger_is_eligible(
    db: &Database,
    shoot_id: i64,
    trigger: SyncTrigger,
) -> SqlResult<bool> {
    match trigger {
        SyncTrigger::TriageComplete => {
            let unreviewed: i64 = db.conn.query_row(
                "SELECT COUNT(*) FROM photos
                 WHERE shoot_id = ?1 AND flag = 'unreviewed'",
                rusqlite::params![shoot_id],
                |r| r.get(0),
            )?;
            Ok(unreviewed == 0)
        }
        SyncTrigger::SelectComplete => {
            if db.unvisited_pick_count(shoot_id)? > 0 {
                return Ok(false);
            }
            let floor = db.get_select_max_floor(shoot_id)?;
            Ok(floor >= 1)
        }
        SyncTrigger::RouteComplete => {
            let routed: i64 = db.conn.query_row(
                "SELECT COUNT(*) FROM photos
                 WHERE shoot_id = ?1 AND flag = 'pick' AND destination != 'unrouted'",
                rusqlite::params![shoot_id],
                |r| r.get(0),
            )?;
            Ok(routed > 0)
        }
    }
}

/// Move every photo in the shoot to its target folder based on current
/// `(flag, destination)`. Idempotent: photos already in the correct spot
/// are skipped. Per-photo failures are recorded in `SyncReport` but
/// don't abort the batch. Every moved file gets a fresh XMP sidecar
/// reflecting current `(rating, flag, destination)` so the on-disk
/// metadata always matches the cull state.
pub fn sync_shoot_layout(db: &Database, shoot_id: i64) -> Result<SyncReport, String> {
    let shoot = db
        .get_shoot(shoot_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("shoot {} not found", shoot_id))?;
    let shoot_root = PathBuf::from(&shoot.dest_path);

    // Bucket folder names come from the global settings (#10) — applied
    // on every sync, so renaming a bucket relocates files on the next
    // run. A fresh DB has no `folder_template` row, which reads back as
    // `Buckets::default()` (`RAW`/`rejects`/`selects`/`edit`/`Export`).
    let buckets: Buckets = db
        .get_settings()
        .map(|s| s.folder_template.buckets)
        .unwrap_or_default();

    let photos = db.photos_for_shoot(shoot_id).map_err(|e| e.to_string())?;
    let mut report = SyncReport::default();

    for p in photos {
        let raw_path = PathBuf::from(&p.raw_path);
        let target_subdir = buckets.subdir_for(&p.flag, &p.destination);
        let target_path = shoot_root.join(&target_subdir).join(&p.filename);

        if raw_path == target_path {
            // File is already at its target folder, but the stars or
            // destination may have changed since the last sync (e.g. the
            // user rated in Select without the file needing to move).
            // Refresh the sidecar so the on-disk XMP always reflects the
            // current DB state — this is what lets us drop the manual
            // "Export XMP sidecars" button.
            if let Err(e) = xmp::write_cull_metadata(&target_path, p.star_rating, &p.destination) {
                report.errors.push(format!(
                    "xmp refresh {}: {}",
                    xmp::sidecar_path(&target_path).display(),
                    e
                ));
            }
            report.skipped_already_placed += 1;
            continue;
        }

        if !raw_path.exists() {
            report.missing.push(p.raw_path.clone());
            continue;
        }
        if target_path.exists() {
            report.collisions.push(target_path.to_string_lossy().into_owned());
            continue;
        }

        if let Some(parent) = target_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                report
                    .errors
                    .push(format!("mkdir {}: {}", parent.display(), e));
                continue;
            }
        }

        if let Err(e) = std::fs::rename(&raw_path, &target_path) {
            report.errors.push(format!(
                "rename {} -> {}: {}",
                raw_path.display(),
                target_path.display(),
                e
            ));
            continue;
        }

        // Sibling JPEG (RAW+JPEG mode) follows the RAW into the same
        // bucket so Capture One/DxO see the pair grouped on disk. We
        // preserve the source extension casing (`.JPG` from Fuji,
        // `.jpg` from others) — Windows is case-insensitive but Linux
        // and macOS-on-APFS case-preserving environments would round-
        // trip a case change as a bogus rename.
        if let Some(ref jpeg_old) = p.sidecar_jpeg_path {
            let jpeg_old_path = PathBuf::from(jpeg_old);
            let jpeg_ext = jpeg_old_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg");
            let jpeg_new_path = target_path.with_extension(jpeg_ext);
            if jpeg_old_path != jpeg_new_path {
                if jpeg_old_path.exists() {
                    if let Err(e) = std::fs::rename(&jpeg_old_path, &jpeg_new_path) {
                        report.errors.push(format!(
                            "rename sibling jpeg {} -> {}: {}",
                            jpeg_old_path.display(),
                            jpeg_new_path.display(),
                            e
                        ));
                    } else if let Err(e) = db.update_sidecar_jpeg_path(
                        p.id,
                        Some(&jpeg_new_path.to_string_lossy()),
                    ) {
                        report.errors.push(format!(
                            "db sidecar_jpeg_path update for {}: {}",
                            p.id, e
                        ));
                    }
                } else {
                    report
                        .missing
                        .push(jpeg_old_path.to_string_lossy().into_owned());
                }
            }
        }

        // Always (re)write the XMP at the destination so the sidecar
        // reflects current cull state (rating + PhotoSift destination;
        // the flag is carried by the folder itself). Clean up any stale
        // sidecar still sitting at the source so we don't orphan
        // contradictory metadata. Errors here don't abort the whole batch:
        // the RAW move already succeeded and the DB update below must run.
        let src_sidecar = xmp::sidecar_path(&raw_path);
        match std::fs::remove_file(&src_sidecar) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => report
                .errors
                .push(format!("rm stale xmp {}: {}", src_sidecar.display(), e)),
        }
        if let Err(e) = xmp::write_cull_metadata(&target_path, p.star_rating, &p.destination) {
            report.errors.push(format!(
                "xmp write {}: {}",
                xmp::sidecar_path(&target_path).display(),
                e
            ));
        }

        let new_raw = target_path.to_string_lossy().into_owned();
        if let Err(e) = db.update_raw_path(p.id, &new_raw) {
            report
                .errors
                .push(format!("db raw_path update for {}: {}", p.id, e));
            // DB and disk disagree now; next sync will still see the old
            // raw_path and try to move a file that isn't there, yielding
            // a "missing" entry. The file_moves log below won't run.
            continue;
        }

        let _ = db.log_file_move(p.id, &p.raw_path, &new_raw);

        report.moved.push(MoveRecord {
            photo_id: p.id,
            from: p.raw_path.clone(),
            to: new_raw,
        });
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{Database, PhotoInsert};
    use std::fs;
    use tempfile::{tempdir, TempDir};

    /// Set up a shoot with photos physically present in `{shoot}/RAW/`
    /// so we can sync and then observe the filesystem.
    fn make_shoot_with_photos(
        db: &mut Database,
        dir: &TempDir,
        filenames: &[&str],
    ) -> (i64, Vec<i64>, PathBuf) {
        let shoot_root = dir.path().join("shoot");
        let raw_dir = shoot_root.join("RAW");
        fs::create_dir_all(&raw_dir).unwrap();

        let shoot_id = db
            .insert_shoot(
                "test",
                "2026-04-21",
                dir.path().to_str().unwrap(),
                shoot_root.to_str().unwrap(),
                "copy",
            )
            .unwrap();

        let mut inserts = Vec::new();
        for (i, name) in filenames.iter().enumerate() {
            let path = raw_dir.join(name);
            fs::write(&path, b"fake raw").unwrap();
            inserts.push(PhotoInsert {
                filename: (*name).into(),
                raw_path: path.to_string_lossy().into_owned(),
                preview_path: format!("/fake/p{}.jpg", i),
                thumb_path: format!("/fake/t{}.jpg", i),
                content_hash: [i as u8 + 1; 32],
                phash: None,
                exif_date: Some("2026-04-15 10:00:00".into()),
                camera: None,
                lens: None,
                focal_length: None,
                aperture: None,
                shutter_speed: None,
                iso: None,
                orientation: None,
                file_size_bytes: None,
                initial_flag: None,
                initial_star_rating: None,
                sidecar_jpeg_path: None,
            });
        }
        let ids = db.insert_photos_batch(shoot_id, &inserts).unwrap();
        (shoot_id, ids, shoot_root)
    }

    // Bucket-name → subdir mapping is covered by `folder_template`'s
    // own unit tests (`default_bucket_subdirs`, `custom_bucket_subdirs`).

    #[test]
    fn sync_distributes_files_by_metadata() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();

        let (shoot_id, ids, shoot_root) = make_shoot_with_photos(
            &mut db,
            &dir,
            &["a.nef", "b.nef", "c.nef", "d.nef", "e.nef"],
        );

        // a stays unreviewed, b rejected, c picked+unrouted (selects),
        // d picked+edit, e picked+export.
        db.set_flag(ids[1], "reject").unwrap();
        db.set_flag(ids[2], "pick").unwrap();
        db.set_flag(ids[3], "pick").unwrap();
        db.set_destination(ids[3], "edit").unwrap();
        db.set_flag(ids[4], "pick").unwrap();
        db.set_destination(ids[4], "export").unwrap();

        let report = sync_shoot_layout(&db, shoot_id).unwrap();
        assert_eq!(report.moved.len(), 4, "4 files should move");
        assert_eq!(report.skipped_already_placed, 1, "unreviewed 'a' stays put");
        assert!(report.errors.is_empty(), "no errors: {:?}", report.errors);

        assert!(shoot_root.join("RAW/a.nef").exists());
        assert!(shoot_root.join("RAW/rejects/b.nef").exists());
        assert!(shoot_root.join("RAW/selects/c.nef").exists());
        assert!(shoot_root.join("RAW/edit/d.nef").exists());
        assert!(shoot_root.join("Export/e.nef").exists());

        // DB rows now point to the new on-disk locations. Match on the
        // literal forward-slash form the subdir constants use — Path::join
        // preserves those slashes verbatim on every platform.
        for (id, sub) in [
            (ids[1], "RAW/rejects"),
            (ids[2], "RAW/selects"),
            (ids[3], "RAW/edit"),
            (ids[4], "Export"),
        ] {
            let row = db.get_photo_by_id(id).unwrap();
            assert!(
                row.raw_path.contains(sub),
                "photo {} raw_path {} should contain {}",
                id,
                row.raw_path,
                sub
            );
        }
    }

    #[test]
    fn sync_refreshes_xmp_when_file_already_placed() {
        // After a rating change in Select, the file doesn't move (it's
        // already in RAW/selects/). The XMP must still be rewritten so
        // the on-disk rating matches the DB.
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, shoot_root) =
            make_shoot_with_photos(&mut db, &dir, &["a.NEF"]);

        db.set_flag(ids[0], "pick").unwrap();
        db.set_star_rating(ids[0], 2).unwrap();
        sync_shoot_layout(&db, shoot_id).unwrap();
        let xmp_path = shoot_root.join("RAW/selects/a.xmp");
        assert!(xmp_path.exists());
        assert!(fs::read_to_string(&xmp_path).unwrap().contains("xmp:Rating=\"2\""));

        // Rating bumped; file stays put. Sync must refresh the sidecar.
        db.set_star_rating(ids[0], 5).unwrap();
        let report = sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(report.moved.is_empty());
        assert_eq!(report.skipped_already_placed, 1);
        assert!(report.errors.is_empty(), "no errors: {:?}", report.errors);
        assert!(fs::read_to_string(&xmp_path).unwrap().contains("xmp:Rating=\"5\""));
    }

    #[test]
    fn sync_is_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, _root) =
            make_shoot_with_photos(&mut db, &dir, &["a.nef", "b.nef"]);
        db.set_flag(ids[0], "reject").unwrap();
        db.set_flag(ids[1], "pick").unwrap();

        let first = sync_shoot_layout(&db, shoot_id).unwrap();
        assert_eq!(first.moved.len(), 2);

        let second = sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(second.moved.is_empty(), "re-sync should move nothing");
        assert_eq!(second.skipped_already_placed, 2);
    }

    #[test]
    fn sync_reverses_when_metadata_flips_back() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, shoot_root) =
            make_shoot_with_photos(&mut db, &dir, &["a.nef"]);
        db.set_flag(ids[0], "reject").unwrap();
        sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(shoot_root.join("RAW/rejects/a.nef").exists());

        // Un-reject → expect the file to flow back on next sync.
        db.set_flag(ids[0], "unreviewed").unwrap();
        let report = sync_shoot_layout(&db, shoot_id).unwrap();
        assert_eq!(report.moved.len(), 1);
        assert!(shoot_root.join("RAW/a.nef").exists());
        assert!(!shoot_root.join("RAW/rejects/a.nef").exists());
    }

    #[test]
    fn sync_writes_fresh_xmp_at_destination() {
        // No source sidecar exists — sync should still write one at the
        // target reflecting the current cull state.
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, shoot_root) =
            make_shoot_with_photos(&mut db, &dir, &["a.NEF"]);

        db.set_flag(ids[0], "reject").unwrap();
        db.set_star_rating(ids[0], 2).unwrap();
        sync_shoot_layout(&db, shoot_id).unwrap();

        assert!(shoot_root.join("RAW/rejects/a.NEF").exists());
        let xmp_path = shoot_root.join("RAW/rejects/a.xmp");
        assert!(xmp_path.exists(), "fresh XMP at destination");
        let content = fs::read_to_string(&xmp_path).unwrap();
        assert!(content.contains("xmp:Rating=\"2\""));
        // The reject state is carried by the folder, not the sidecar —
        // emitting xmp:Label would surface as a color tag in Capture One
        // and DxO that the user never actually chose.
        assert!(
            !content.contains("xmp:Label"),
            "reject sidecar must not write xmp:Label: {}",
            content
        );
    }

    #[test]
    fn sync_cleans_up_stale_source_xmp() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, shoot_root) =
            make_shoot_with_photos(&mut db, &dir, &["a.NEF"]);

        // Pre-existing stale sidecar at the source.
        let src_sidecar = shoot_root.join("RAW/a.xmp");
        fs::write(&src_sidecar, b"<xmp/>").unwrap();

        db.set_flag(ids[0], "reject").unwrap();
        sync_shoot_layout(&db, shoot_id).unwrap();

        assert!(!src_sidecar.exists(), "stale source sidecar removed");
        let dst_sidecar = shoot_root.join("RAW/rejects/a.xmp");
        assert!(dst_sidecar.exists(), "fresh sidecar written at target");
        // New content reflects cull metadata, not the `<xmp/>` stub.
        let content = fs::read_to_string(&dst_sidecar).unwrap();
        assert!(content.contains("photosift:destination"));
    }

    #[test]
    fn sync_rewrites_xmp_when_metadata_changes() {
        // Flip the flag twice; each sync should produce an XMP at the
        // new folder reflecting the current state.
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, shoot_root) =
            make_shoot_with_photos(&mut db, &dir, &["a.NEF"]);

        db.set_flag(ids[0], "reject").unwrap();
        sync_shoot_layout(&db, shoot_id).unwrap();
        let rejects_xmp = shoot_root.join("RAW/rejects/a.xmp");
        assert!(rejects_xmp.exists());

        db.set_flag(ids[0], "pick").unwrap();
        sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(!rejects_xmp.exists(), "old xmp cleared from rejects");
        let selects_xmp = shoot_root.join("RAW/selects/a.xmp");
        assert!(selects_xmp.exists(), "new xmp in selects");
        let content = fs::read_to_string(&selects_xmp).unwrap();
        assert!(content.contains("photosift:destination=\"unrouted\""));
        assert!(!content.contains("xmp:Label"));
    }

    #[test]
    fn sync_reports_missing_source() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, shoot_root) =
            make_shoot_with_photos(&mut db, &dir, &["a.nef"]);
        db.set_flag(ids[0], "reject").unwrap();
        fs::remove_file(shoot_root.join("RAW/a.nef")).unwrap(); // user moved it in Explorer

        let report = sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(report.moved.is_empty());
        assert_eq!(report.missing.len(), 1);
        // DB row untouched so a later re-sync can retry.
        let row = db.get_photo_by_id(ids[0]).unwrap();
        assert!(row.raw_path.ends_with("a.nef"));
        assert!(row.raw_path.contains("RAW"));
    }

    #[test]
    fn sync_reports_collision_and_preserves_existing() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, shoot_root) =
            make_shoot_with_photos(&mut db, &dir, &["a.nef"]);
        db.set_flag(ids[0], "reject").unwrap();

        // Pre-create the target with distinct contents.
        fs::create_dir_all(shoot_root.join("RAW/rejects")).unwrap();
        fs::write(shoot_root.join("RAW/rejects/a.nef"), b"pre-existing").unwrap();

        let report = sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(report.moved.is_empty());
        assert_eq!(report.collisions.len(), 1);
        assert_eq!(
            fs::read(shoot_root.join("RAW/rejects/a.nef")).unwrap(),
            b"pre-existing"
        );
        // Source still there, untouched.
        assert!(shoot_root.join("RAW/a.nef").exists());
    }

    #[test]
    fn trigger_eligibility_matches_gate_semantics() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();
        let (shoot_id, ids, _root) =
            make_shoot_with_photos(&mut db, &dir, &["a.nef", "b.nef"]);

        // Initial: a+b unreviewed, no picks, no ratings.
        assert!(!trigger_is_eligible(&db, shoot_id, SyncTrigger::TriageComplete).unwrap());
        assert!(!trigger_is_eligible(&db, shoot_id, SyncTrigger::SelectComplete).unwrap());
        assert!(!trigger_is_eligible(&db, shoot_id, SyncTrigger::RouteComplete).unwrap());

        // Clear unreviewed → triage eligible.
        db.set_flag(ids[0], "pick").unwrap();
        db.set_flag(ids[1], "reject").unwrap();
        assert!(trigger_is_eligible(&db, shoot_id, SyncTrigger::TriageComplete).unwrap());

        // Select: need picks visited AND floor >= 1.
        assert!(!trigger_is_eligible(&db, shoot_id, SyncTrigger::SelectComplete).unwrap());
        db.mark_select_visited(ids[0]).unwrap();
        assert!(
            !trigger_is_eligible(&db, shoot_id, SyncTrigger::SelectComplete).unwrap(),
            "floor still 0"
        );
        db.bump_select_max_floor(shoot_id, 1).unwrap();
        assert!(trigger_is_eligible(&db, shoot_id, SyncTrigger::SelectComplete).unwrap());

        // Route: need at least one routed pick.
        assert!(!trigger_is_eligible(&db, shoot_id, SyncTrigger::RouteComplete).unwrap());
        db.set_destination(ids[0], "edit").unwrap();
        assert!(trigger_is_eligible(&db, shoot_id, SyncTrigger::RouteComplete).unwrap());
    }

    #[test]
    fn bump_select_max_floor_is_monotonic() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        let shoot_id = db
            .insert_shoot("t", "2026-04-15", "/s", "/d", "copy")
            .unwrap();
        assert_eq!(db.get_select_max_floor(shoot_id).unwrap(), 0);
        db.bump_select_max_floor(shoot_id, 2).unwrap();
        assert_eq!(db.get_select_max_floor(shoot_id).unwrap(), 2);
        db.bump_select_max_floor(shoot_id, 1).unwrap(); // must not decrease
        assert_eq!(db.get_select_max_floor(shoot_id).unwrap(), 2);
        db.bump_select_max_floor(shoot_id, 5).unwrap();
        assert_eq!(db.get_select_max_floor(shoot_id).unwrap(), 5);
    }

    /// RAW+JPEG mode: when a photo carries `sidecar_jpeg_path`, layout
    /// sync moves the JPEG into the same bucket as the RAW and updates
    /// the DB column to the new path.
    #[test]
    fn sync_moves_sibling_jpeg_with_raw() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();

        let shoot_root = dir.path().join("shoot");
        let raw_dir = shoot_root.join("RAW");
        fs::create_dir_all(&raw_dir).unwrap();

        let shoot_id = db
            .insert_shoot(
                "test",
                "2026-04-21",
                dir.path().to_str().unwrap(),
                shoot_root.to_str().unwrap(),
                "copy",
            )
            .unwrap();

        let raw_src = raw_dir.join("DSCF0123.RAF");
        let jpeg_src = raw_dir.join("DSCF0123.JPG");
        fs::write(&raw_src, b"fake raf").unwrap();
        fs::write(&jpeg_src, b"fake jpg").unwrap();

        let insert = PhotoInsert {
            filename: "DSCF0123.RAF".into(),
            raw_path: raw_src.to_string_lossy().into_owned(),
            preview_path: "/fake/p.jpg".into(),
            thumb_path: "/fake/t.jpg".into(),
            content_hash: [9u8; 32],
            phash: None,
            exif_date: Some("2026-04-15 10:00:00".into()),
            camera: None,
            lens: None,
            focal_length: None,
            aperture: None,
            shutter_speed: None,
            iso: None,
            orientation: None,
            file_size_bytes: None,
            initial_flag: None,
            initial_star_rating: None,
            sidecar_jpeg_path: Some(jpeg_src.to_string_lossy().into_owned()),
        };
        let ids = db.insert_photos_batch(shoot_id, &[insert]).unwrap();
        let photo_id = ids[0];

        db.set_flag(photo_id, "reject").unwrap();

        let report = sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert_eq!(report.moved.len(), 1, "RAW move recorded");

        // Layout module joins subdir + filename in two steps, leaving
        // the embedded forward slashes from `Buckets::subdir_for` intact.
        // Match that construction so string equality holds.
        let raw_target = shoot_root.join("RAW/rejects").join("DSCF0123.RAF");
        let jpeg_target = shoot_root.join("RAW/rejects").join("DSCF0123.JPG");
        assert!(raw_target.exists(), "RAW moved to rejects/");
        assert!(jpeg_target.exists(), "JPEG followed RAW into rejects/");
        assert!(!raw_src.exists(), "old RAW gone");
        assert!(!jpeg_src.exists(), "old JPEG gone");

        let row = db.get_photo_by_id(photo_id).unwrap();
        assert_eq!(row.raw_path, raw_target.to_string_lossy());
        assert_eq!(
            row.sidecar_jpeg_path.as_deref(),
            Some(jpeg_target.to_string_lossy().as_ref()),
            "DB sidecar_jpeg_path tracks the new JPEG location"
        );

        // Idempotent — second run is a no-op for the JPEG too.
        let report2 = sync_shoot_layout(&db, shoot_id).unwrap();
        assert_eq!(report2.moved.len(), 0);
        assert_eq!(report2.skipped_already_placed, 1);
        assert!(report2.errors.is_empty());
    }

    /// Routing a RAW+JPEG pick to Export lands both files in the
    /// top-level `Export/` folder (not `RAW/export/`), with a sidecar,
    /// and the move reverses cleanly when the destination flips back.
    #[test]
    fn sync_routes_paired_pick_to_export_bucket() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(&db_path).unwrap();

        let shoot_root = dir.path().join("shoot");
        let raw_dir = shoot_root.join("RAW");
        fs::create_dir_all(&raw_dir).unwrap();

        let shoot_id = db
            .insert_shoot(
                "test",
                "2026-05-06",
                dir.path().to_str().unwrap(),
                shoot_root.to_str().unwrap(),
                "copy",
            )
            .unwrap();

        let raw_src = raw_dir.join("DSC_0042.NEF");
        let jpeg_src = raw_dir.join("DSC_0042.JPG");
        fs::write(&raw_src, b"fake nef").unwrap();
        fs::write(&jpeg_src, b"fake jpg").unwrap();

        let insert = PhotoInsert {
            filename: "DSC_0042.NEF".into(),
            raw_path: raw_src.to_string_lossy().into_owned(),
            preview_path: "/fake/p.jpg".into(),
            thumb_path: "/fake/t.jpg".into(),
            content_hash: [7u8; 32],
            phash: None,
            exif_date: Some("2026-05-06 12:00:00".into()),
            camera: None,
            lens: None,
            focal_length: None,
            aperture: None,
            shutter_speed: None,
            iso: None,
            orientation: None,
            file_size_bytes: None,
            initial_flag: None,
            initial_star_rating: None,
            sidecar_jpeg_path: Some(jpeg_src.to_string_lossy().into_owned()),
        };
        let ids = db.insert_photos_batch(shoot_id, &[insert]).unwrap();
        let photo_id = ids[0];

        db.set_flag(photo_id, "pick").unwrap();
        db.set_destination(photo_id, "export").unwrap();

        let report = sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert_eq!(report.moved.len(), 1);

        let raw_target = shoot_root.join("Export").join("DSC_0042.NEF");
        let jpeg_target = shoot_root.join("Export").join("DSC_0042.JPG");
        let xmp_target = shoot_root.join("Export").join("DSC_0042.xmp");
        assert!(raw_target.exists(), "RAW moved to Export/");
        assert!(jpeg_target.exists(), "sibling JPEG followed into Export/");
        assert!(xmp_target.exists(), "sidecar written in Export/");
        assert!(
            !shoot_root.join("RAW/export").exists(),
            "no legacy RAW/export/ bucket"
        );
        let xmp = fs::read_to_string(&xmp_target).unwrap();
        assert!(xmp.contains("photosift:destination=\"export\""));
        assert!(!xmp.contains("xmp:Label"));

        // Flip back to unrouted → both files flow into RAW/selects/.
        db.set_destination(photo_id, "unrouted").unwrap();
        let report2 = sync_shoot_layout(&db, shoot_id).unwrap();
        assert_eq!(report2.moved.len(), 1);
        assert!(shoot_root.join("RAW/selects/DSC_0042.NEF").exists());
        assert!(shoot_root.join("RAW/selects/DSC_0042.JPG").exists());
        assert!(!raw_target.exists(), "Export/ RAW gone after reverse");
        assert!(!jpeg_target.exists(), "Export/ JPEG gone after reverse");
        assert!(!xmp_target.exists(), "Export/ sidecar cleaned up after reverse");
    }
}
