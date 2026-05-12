pub mod clustering;
pub mod copy;
pub mod hashing;
pub mod pairing;
pub mod phash;
pub mod preview;
pub mod progress;
pub mod thumbnail;
pub mod walker;

use crate::db::schema::{Database, PhotoInsert};
use crate::metadata::{exif, orientation, xmp};
use pairing::ImportItem;
use progress::{ImportComplete, ImportPhase, ImportPhotoReady, ImportProgress};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

#[derive(Debug)]
enum ProcessedFile {
    Ingested(IngestedFile),
    Skipped,
}

#[derive(Debug)]
struct IngestedFile {
    insert: PhotoInsert,
    preview_bytes: Vec<u8>,
    thumb_bytes: Option<Vec<u8>>,
    phash: Option<[u8; 8]>,
}

/// Import mode: either copy files into a canonical library folder, or
/// register them in-place (leaves the source directory untouched).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Copy,
    InPlace,
}

impl ImportMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportMode::Copy => "copy",
            ImportMode::InPlace => "in_place",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "in_place" => ImportMode::InPlace,
            _ => ImportMode::Copy,
        }
    }
}

/// Main import orchestrator. Runs on a background thread.
/// Opens its own DB connection (WAL mode allows concurrent access).
pub fn run_import(
    app: AppHandle,
    source: PathBuf,
    slug: String,
    import_mode: ImportMode,
    cancel: Arc<AtomicBool>,
    selected_paths: Option<Vec<PathBuf>>,
) -> Result<i64, String> {
    let db = Database::open_global().map_err(|e| e.to_string())?;
    let db = Mutex::new(db);
    // Phase 1: Walk source
    emit_progress(&app, 0, ImportPhase::Walking, 0, 0, "");
    let files = walker::walk_source(&source);
    // Group RAW + sibling-JPEG pairs so a RAW+JPEG folder produces one
    // photo row per frame (the JPEG rides on the RAW's row).
    let mut items = pairing::pair(files);
    // Pre-import selection filter: when the user cherry-picked from the
    // scan dialog we intersect on absolute paths. Preserves walker order
    // so counters and progress events still make sense. Selection paths
    // come from the scan dialog, which sees the flat file list — so a
    // selected RAW path matches its ImportItem; a selected JPEG that's
    // paired to a RAW will not match (the pair is keyed on the RAW).
    if let Some(selected) = selected_paths {
        use std::collections::HashSet;
        let wanted: HashSet<PathBuf> = selected.into_iter().collect();
        items.retain(|item| wanted.contains(item.primary_path()));
    }
    if items.is_empty() {
        return Err("No supported image files found in source directory".into());
    }
    let total = items.len();

    // Phase 2: Probe first file for EXIF date to derive YYYY-MM
    let yyyy_mm = derive_yyyy_mm(items[0].primary_path());

    // Phase 3: Create shoot row and directories.
    // Copy mode derives a canonical folder under the user's library root;
    // in-place mode registers files where they are and records the source
    // folder as the effective dest_path.
    let (configured_lib_root, folder_template): (Option<PathBuf>, crate::folder_template::FolderTemplate) = {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        match db_guard.get_settings() {
            Ok(s) => (s.library_root.map(PathBuf::from), s.folder_template),
            Err(_) => (None, crate::folder_template::FolderTemplate::default()),
        }
    };
    let raw_bucket = folder_template.buckets.raw.clone();
    let shoot_dir = match import_mode {
        ImportMode::Copy => {
            let lib_root = configured_lib_root.unwrap_or_else(copy::library_root);
            copy::shoot_folder(&folder_template, &lib_root, &yyyy_mm, &slug)
        }
        ImportMode::InPlace => source.clone(),
    };
    let dest_path = shoot_dir.to_string_lossy().to_string();

    let shoot_id = {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        let date = format!(
            "{}-01",
            &yyyy_mm
        );
        db_guard
            .insert_shoot(&slug, &date, &source.to_string_lossy(), &dest_path, import_mode.as_str())
            .map_err(|e| e.to_string())?
    };

    let cache_dir = crate::db::schema::shoot_cache_dir(shoot_id);
    let previews_dir = cache_dir.join("previews");
    let thumbs_dir = cache_dir.join("thumbs");
    std::fs::create_dir_all(&previews_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&thumbs_dir).map_err(|e| e.to_string())?;

    // Phase 4: Per-file parallel pipeline
    let counter = AtomicUsize::new(0);
    let app_ref = &app;

    let results: Vec<ProcessedFile> = items
        .par_iter()
        .map(|item| {
            if cancel.load(Ordering::Relaxed) {
                return ProcessedFile::Skipped;
            }

            let result = process_one_file(
                item,
                &shoot_dir,
                &raw_bucket,
                &previews_dir,
                &thumbs_dir,
                &db,
                import_mode,
                &cancel,
            );

            let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
            let fname = item
                .primary_path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            emit_progress(app_ref, shoot_id, ImportPhase::Processing, n, total, &fname);

            result
        })
        .collect();

    // Phase 5: Sequential DB insert
    emit_progress(&app, shoot_id, ImportPhase::Finalizing, 0, total, "");

    let mut inserts = Vec::new();
    let mut file_data = Vec::new();
    let mut dedup_skipped = 0usize;

    for r in results {
        match r {
            ProcessedFile::Ingested(f) => {
                inserts.push(f.insert);
                file_data.push((f.preview_bytes, f.thumb_bytes, f.phash));
            }
            ProcessedFile::Skipped => {
                dedup_skipped += 1;
            }
        }
    }

    let photo_ids = {
        let mut db_guard = db.lock().map_err(|e| e.to_string())?;
        db_guard
            .insert_photos_batch(shoot_id, &inserts)
            .map_err(|e| e.to_string())?
    };

    // Seed the shoot cover with the first imported photo. A later
    // AI-pick pass can overwrite via `force_set_shoot_cover`; the
    // `_if_unset` variant won't clobber a user- or AI-chosen cover.
    if let Some(&first_id) = photo_ids.first() {
        if let Ok(db_guard) = db.lock() {
            let _ = db_guard.set_shoot_cover_if_unset(shoot_id, first_id);
        }
    }

    // Write preview/thumb files and update paths (now that we have photo_ids).
    // After each photo has disk files + DB paths, emit `import-photo-ready` so
    // the shoot list can show live progress without polling. Total here is the
    // post-dedup count — it matches what the UI will eventually see, so a "42/198"
    // counter stays monotonic instead of dropping when skipped files settle.
    let photos_total = photo_ids.len();
    {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        for (i, &photo_id) in photo_ids.iter().enumerate() {
            let preview_path = previews_dir.join(format!("{}.jpg", photo_id));
            let thumb_path = thumbs_dir.join(format!("{}.jpg", photo_id));

            if let Err(e) = std::fs::write(&preview_path, &file_data[i].0) {
                log::error!("Failed to write preview for photo {}: {}", photo_id, e);
            }
            if let Some(ref thumb) = file_data[i].1 {
                if let Err(e) = std::fs::write(&thumb_path, thumb) {
                    log::error!("Failed to write thumbnail for photo {}: {}", photo_id, e);
                }
            }

            let thumb_path_str = if file_data[i].1.is_some() {
                thumb_path.to_string_lossy().to_string()
            } else {
                String::new()
            };

            if let Err(e) = db_guard.update_photo_paths(
                photo_id,
                &preview_path.to_string_lossy(),
                &thumb_path_str,
            ) {
                log::error!("Failed to update paths for photo {}: {}", photo_id, e);
            }

            let _ = app.emit(
                "import-photo-ready",
                ImportPhotoReady {
                    shoot_id,
                    photo_id,
                    filename: inserts[i].filename.clone(),
                    imported: i + 1,
                    total: photos_total,
                },
            );
        }
    }

    // Phase 6: Clustering
    emit_progress(&app, shoot_id, ImportPhase::Clustering, 0, total, "");

    // Pull phash + capture time from the DB now that photos are
    // persisted, so the time-window check reads the same parsed
    // timestamp reclustering uses later.
    let settings = {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        db_guard.get_settings().unwrap_or_default()
    };
    let phash_rows = {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        db_guard
            .phashes_for_shoot(shoot_id)
            .map_err(|e| e.to_string())?
    };
    let groups = clustering::cluster_phashes(
        &phash_rows,
        settings.near_dup_threshold as u32,
        settings.related_threshold as u32,
        settings.group_time_window_s.max(0) as u32,
    );

    {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        for group in &groups {
            let group_id = db_guard
                .create_group(shoot_id, group.group_type)
                .map_err(|e| e.to_string())?;

            for (i, &idx) in group.member_indices.iter().enumerate() {
                let photo_id = phash_rows[idx].0;
                db_guard
                    .add_group_member(group_id, photo_id, i == 0)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    // Groups are now persisted. Tell a progressive cull view subscriber to
    // refetch so the newly-imported photos appear with their cluster
    // membership instead of as singletons.
    let _ = app.emit(
        "shoot-groups-updated",
        serde_json::json!({ "shootId": shoot_id }),
    );

    // Update photo count
    let photo_count = photo_ids.len();
    {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        db_guard
            .update_shoot_photo_count(shoot_id, photo_count as i64)
            .map_err(|e| e.to_string())?;
    }

    let _ = app.emit(
        "import-complete",
        ImportComplete {
            shoot_id,
            photo_count,
            dedup_skipped,
        },
    );

    Ok(shoot_id)
}

fn process_one_file(
    item: &ImportItem,
    shoot_dir: &Path,
    raw_bucket: &str,
    previews_dir: &Path,
    thumbs_dir: &Path,
    db: &Mutex<Database>,
    import_mode: ImportMode,
    cancel: &Arc<AtomicBool>,
) -> ProcessedFile {
    let t_start = Instant::now();

    if cancel.load(Ordering::Relaxed) {
        return ProcessedFile::Skipped;
    }

    // The "primary" path is the RAW for paired/lone-RAW items and the
    // JPEG for standalone JPEGs. Sibling JPEG (RAW+JPEG mode only) is
    // tracked separately and follows the RAW through layout moves.
    let src_path: &Path = item.primary_path();
    let src_sibling_jpeg: Option<&Path> = match item {
        ImportItem::RawWithSibling { jpeg, .. } => Some(jpeg.as_path()),
        _ => None,
    };

    // 0. File size — cheap stat used both as the heuristic-dedup signal
    //    and persisted on the row for future SD-card scans.
    let file_size_bytes = std::fs::metadata(src_path).ok().map(|m| m.len());

    let filename = src_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // 1. EXIF (cheap — reads only the file headers, not the pixel data).
    // For paired RAW+JPEG items prefer the JPEG: it carries clean
    // standard EXIF that kamadak-exif handles without the RAF detour,
    // and the camera writes the same DateTimeOriginal to both files
    // anyway. Lone RAFs go through extract_exif's RAF-aware branch.
    let t_exif = Instant::now();
    let exif_source: &Path = src_sibling_jpeg.unwrap_or(src_path);
    let exif_data = exif::extract_exif(exif_source).ok();
    let exif_ms = t_exif.elapsed().as_secs_f64() * 1000.0;

    // 1b. Look for an existing XMP sidecar next to the source file. If present,
    // prefer its rating/label over EXIF (XMP is the more recently-written
    // metadata when the user has culled in another tool like DxO or C1).
    let sidecar_rating = xmp::read_rating(src_path);
    let sidecar_flag = xmp::read_flag_from_label(src_path);
    let initial_star_rating = sidecar_rating
        .or_else(|| exif_data.as_ref().and_then(|e| e.rating));
    let initial_flag = sidecar_flag;

    if cancel.load(Ordering::Relaxed) {
        return ProcessedFile::Skipped;
    }

    // 2. Copy + SHA-256 in a single pass — reads the source file (often
    // on a slow SD card) exactly once. In-place mode skips the copy and
    // hashes the source directly.
    let t_copy_hash = Instant::now();
    let (raw_path, content_hash, copy_made) = match import_mode {
        ImportMode::Copy => {
            let dest = copy::plan_dest(shoot_dir, raw_bucket, &filename);
            match copy::copy_with_hash(src_path, &dest) {
                Ok((p, h)) => (p, h, true),
                Err(e) => {
                    log::error!("Copy+hash failed for {:?}: {}", src_path, e);
                    return ProcessedFile::Skipped;
                }
            }
        }
        ImportMode::InPlace => match hashing::sha256_stream(src_path) {
            Ok(h) => (src_path.to_path_buf(), h, false),
            Err(e) => {
                log::error!("SHA-256 failed for {:?}: {}", src_path, e);
                return ProcessedFile::Skipped;
            }
        },
    };
    let copy_hash_ms = t_copy_hash.elapsed().as_secs_f64() * 1000.0;

    // 3. Dedup check on the just-computed hash. If a duplicate slipped past
    // the heuristic dedup at scan time, undo the copy we just made.
    if let Ok(guard) = db.lock() {
        if let Ok(Some(_)) = guard.photo_exists_by_hash(&content_hash) {
            if copy_made {
                let _ = std::fs::remove_file(&raw_path);
            }
            return ProcessedFile::Skipped;
        }
    }

    if cancel.load(Ordering::Relaxed) {
        if copy_made {
            let _ = std::fs::remove_file(&raw_path);
        }
        return ProcessedFile::Skipped;
    }

    // 3b. Sibling JPEG handling (RAW+JPEG mode). In Copy mode the JPEG
    // is placed next to the RAW with a matching stem so layout moves
    // and downstream tools (Capture One, DxO) see the pair grouped.
    // In-place mode just records the source path. Failure here is
    // non-fatal — we lose the sibling but still keep the RAW row.
    let local_jpeg_path: Option<PathBuf> = match (src_sibling_jpeg, import_mode) {
        (Some(src_jpeg), ImportMode::Copy) => {
            let jpeg_ext = src_jpeg
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg");
            let jpeg_dest = raw_path.with_extension(jpeg_ext);
            match std::fs::copy(src_jpeg, &jpeg_dest) {
                Ok(_) => Some(jpeg_dest),
                Err(e) => {
                    log::warn!(
                        "Sibling JPEG copy failed for {:?}: {} (importing RAW only)",
                        src_jpeg,
                        e
                    );
                    None
                }
            }
        }
        (Some(src_jpeg), ImportMode::InPlace) => Some(src_jpeg.to_path_buf()),
        (None, _) => None,
    };

    // 4. Extract preview bytes + decode for thumb/pHash. When a sibling
    // JPEG is available it's the preferred source: camera JPEGs are
    // higher quality, color-processed, and skip the embedded-preview
    // dance entirely. Fall back to the RAW's embedded JPEG otherwise.
    let t_preview = Instant::now();
    let preview_source: &Path = local_jpeg_path.as_deref().unwrap_or(&raw_path);
    let (preview_bytes, decoded) = match preview::extract_and_decode(preview_source) {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "JPEG extraction failed for {:?}: {}",
                preview_source,
                e
            );
            if copy_made {
                let _ = std::fs::remove_file(&raw_path);
                if let Some(ref jp) = local_jpeg_path {
                    let _ = std::fs::remove_file(jp);
                }
            }
            return ProcessedFile::Skipped;
        }
    };
    let preview_ms = t_preview.elapsed().as_secs_f64() * 1000.0;

    if cancel.load(Ordering::Relaxed) {
        if copy_made {
            let _ = std::fs::remove_file(&raw_path);
            if let Some(ref jp) = local_jpeg_path {
                let _ = std::fs::remove_file(jp);
            }
        }
        return ProcessedFile::Skipped;
    }

    // 5b. If EXIF says the camera was rotated, upright both the decoded
    // image and the on-disk preview bytes now. Downstream consumers
    // (thumbnail, pHash, AI worker reading from disk) then all agree on
    // the same rotated-space coordinates.
    let t_rotate = Instant::now();
    let orientation_tag = exif_data.as_ref().and_then(|e| e.orientation);
    let (preview_bytes, decoded) = match decoded {
        Some(img) => match orientation::apply_and_reencode(img, orientation_tag, preview_bytes) {
            Ok((bytes, rotated)) => (bytes, Some(rotated)),
            Err(e) => {
                log::error!(
                    "Orientation apply failed for {:?}: {} (skipping)",
                    raw_path,
                    e
                );
                return ProcessedFile::Skipped;
            }
        },
        None => (preview_bytes, None),
    };
    let rotate_ms = t_rotate.elapsed().as_secs_f64() * 1000.0;

    let t_thumb_phash = Instant::now();
    let (thumb_bytes, phash_val) = match decoded {
        Some(img) => {
            let thumb = thumbnail::make_thumb(&img).ok();
            let ph = Some(phash::compute_phash(&img));
            (thumb, ph)
        }
        None => (None, None),
    };
    let thumb_phash_ms = t_thumb_phash.elapsed().as_secs_f64() * 1000.0;

    let total_ms = t_start.elapsed().as_secs_f64() * 1000.0;
    log::info!(
        "ingest::process_one_file {} total={:.1}ms (exif={:.1} copy+hash={:.1} preview={:.1} rotate={:.1} thumb+phash={:.1})",
        filename,
        total_ms,
        exif_ms,
        copy_hash_ms,
        preview_ms,
        rotate_ms,
        thumb_phash_ms,
    );

    let _ = previews_dir;
    let _ = thumbs_dir;

    let insert = PhotoInsert {
        filename,
        raw_path: raw_path.to_string_lossy().to_string(),
        preview_path: String::new(),
        thumb_path: String::new(),
        content_hash,
        phash: phash_val,
        exif_date: exif_data.as_ref().and_then(|e| e.capture_time.clone()),
        camera: exif_data.as_ref().and_then(|e| e.camera_model.clone()),
        lens: exif_data.as_ref().and_then(|e| e.lens.clone()),
        focal_length: exif_data.as_ref().and_then(|e| e.focal_length),
        aperture: exif_data.as_ref().and_then(|e| e.aperture),
        shutter_speed: exif_data.as_ref().and_then(|e| e.shutter_speed.clone()),
        iso: exif_data.as_ref().and_then(|e| e.iso),
        orientation: orientation_tag,
        file_size_bytes,
        initial_flag,
        initial_star_rating,
        sidecar_jpeg_path: local_jpeg_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    };

    ProcessedFile::Ingested(IngestedFile {
        insert,
        preview_bytes,
        thumb_bytes,
        phash: phash_val,
    })
}

fn derive_yyyy_mm(first_file: &Path) -> String {
    if let Ok(ed) = exif::extract_exif(first_file) {
        if let Some(ref dt) = ed.capture_time {
            // EXIF date format: "2026-04-15 10:30:00" or "2026:04:15 10:30:00"
            let clean = dt.replace(':', "-");
            if clean.len() >= 7 {
                let yyyy = &clean[..4];
                let mm = &clean[5..7];
                return format!("{}-{}", yyyy, mm);
            }
        }
    }
    // Fallback: file modification time
    if let Ok(meta) = std::fs::metadata(first_file) {
        if let Ok(modified) = meta.modified() {
            let dt: chrono::DateTime<chrono::Local> = modified.into();
            return dt.format("%Y-%m").to_string();
        }
    }
    chrono::Local::now().format("%Y-%m").to_string()
}

fn emit_progress(
    app: &AppHandle,
    shoot_id: i64,
    phase: ImportPhase,
    current: usize,
    total: usize,
    filename: &str,
) {
    let _ = app.emit(
        "import-progress",
        ImportProgress {
            shoot_id,
            phase,
            current,
            total,
            current_filename: filename.to_string(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout;
    use crate::metadata::exif::tests::{make_jpeg_with_exif, make_synthetic_raf};
    use tempfile::TempDir;

    /// End-to-end: walker + pair + process_one_file + insert_photos_batch
    /// + sync_shoot_layout. This is the closest we can get to invoking
    /// `run_import` itself without a Tauri AppHandle — every other piece
    /// of the import pipeline runs against real on-disk files.
    ///
    /// Verifies the four contracts the user cares about:
    ///   (a) RAW+JPG pair → ONE photo row with sidecar_jpeg_path set
    ///   (b) lone JPG → its own row, no sidecar
    ///   (c) flagging the paired photo as reject moves BOTH the RAW
    ///       and the JPG into RAW/rejects/, with sidecar_jpeg_path on
    ///       the row updated to track the move
    ///   (d) sync is idempotent — re-running it after the move is a
    ///       no-op
    #[test]
    fn end_to_end_paired_raf_import_then_layout_move() {
        // --- (1) Source folder with one paired RAF+JPG and one lone JPG.
        let source = TempDir::new().unwrap();
        let src_raf = source.path().join("DSCF0001.RAF");
        let src_jpg = source.path().join("DSCF0001.JPG");
        let src_lone = source.path().join("standalone.jpg");
        // Synthetic RAF body: magic + a JPEG-with-no-EXIF padded above
        // the 10KB embedded-JPEG threshold so the preview pipeline can
        // pull bytes out without erroring.
        let mut raf_body = Vec::from(b"FUJIFILMCCD-RAW \0".as_slice());
        raf_body.extend_from_slice(&[0xFFu8, 0xD8]); // SOI
        raf_body.extend_from_slice(&[0xFFu8, 0xFE]); // COM
        let pad = 12_000usize;
        raf_body.extend_from_slice(&((pad + 2) as u16).to_be_bytes());
        raf_body.resize(raf_body.len() + pad, 0u8);
        raf_body.extend_from_slice(&[0xFFu8, 0xD9]); // EOI
        std::fs::write(&src_raf, &raf_body).unwrap();
        std::fs::write(
            &src_jpg,
            make_jpeg_with_exif("2024:08:02 09:15:00", "FUJIFILM X-T5"),
        )
        .unwrap();
        std::fs::write(
            &src_lone,
            make_jpeg_with_exif("2024:01:15 14:30:00", "NIKON D750"),
        )
        .unwrap();

        // --- (2) Walk the source dir and apply the pairing rules.
        let files = walker::walk_source(source.path());
        let items = pairing::pair(files);
        assert_eq!(items.len(), 2, "RAF+JPG collapses, lone JPG stays = 2");

        // --- (3) Set up DB + a shoot whose dest_path is a real folder
        // process_one_file can copy into.
        let db_dir = TempDir::new().unwrap();
        let db_path = db_dir.path().join("e2e.sqlite");
        let mut db = Database::open(&db_path).unwrap();
        // process_one_file calls copy::plan_dest, which appends "RAW/"
        // to the shoot_dir argument. Pass the shoot ROOT, not the RAW
        // subdir, or we get {shoot}/RAW/RAW/ nesting.
        let shoot_root = db_dir.path().join("shoot");
        std::fs::create_dir_all(&shoot_root).unwrap();
        let raw_dir = shoot_root.join("RAW"); // for assertions only
        let previews_dir = db_dir.path().join("previews");
        let thumbs_dir = db_dir.path().join("thumbs");
        std::fs::create_dir_all(&previews_dir).unwrap();
        std::fs::create_dir_all(&thumbs_dir).unwrap();
        let shoot_id = db
            .insert_shoot(
                "e2e",
                "2024-08-01",
                source.path().to_str().unwrap(),
                shoot_root.to_str().unwrap(),
                "copy",
            )
            .unwrap();

        // --- (4) Run process_one_file on each ImportItem. Mirrors what
        // run_import's parallel block does, minus the rayon + AppHandle.
        let db_mutex = Mutex::new(db);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut inserts = Vec::new();
        for item in &items {
            match process_one_file(
                item,
                &shoot_root,
                "RAW",
                &previews_dir,
                &thumbs_dir,
                &db_mutex,
                ImportMode::Copy,
                &cancel,
            ) {
                ProcessedFile::Ingested(f) => inserts.push(f.insert),
                ProcessedFile::Skipped => panic!("process_one_file skipped {:?}", item),
            }
        }
        let mut db = db_mutex.into_inner().unwrap();

        // --- (5) The PhotoInsert built for the paired item must carry
        // sidecar_jpeg_path; the lone item's must be None. (Contract a/b)
        let paired_insert = inserts
            .iter()
            .find(|i| i.filename.eq_ignore_ascii_case("DSCF0001.RAF"))
            .expect("paired insert");
        let lone_insert = inserts
            .iter()
            .find(|i| i.filename.eq_ignore_ascii_case("standalone.jpg"))
            .expect("lone insert");
        assert!(
            paired_insert.sidecar_jpeg_path.is_some(),
            "paired RAF row must carry sidecar_jpeg_path"
        );
        assert_eq!(
            lone_insert.sidecar_jpeg_path, None,
            "lone JPG row must NOT carry sidecar_jpeg_path"
        );
        // EXIF for the paired RAF was read from the JPG sibling, so
        // capture_time and camera should be populated even though the
        // RAF body has no EXIF.
        assert!(paired_insert.exif_date.is_some());
        assert!(
            paired_insert
                .camera
                .as_deref()
                .map(|c| c.contains("X-T5"))
                .unwrap_or(false),
            "paired RAF camera must come through the JPG: {:?}",
            paired_insert.camera,
        );

        // --- (6) Persist the inserts. The post-insert DB shape is what
        // the cull views, AI worker, and layout sync all read.
        let ids = db.insert_photos_batch(shoot_id, &inserts).unwrap();
        assert_eq!(ids.len(), 2);

        // Files actually copied to the shoot directory by Copy mode.
        assert!(
            raw_dir.join("DSCF0001.RAF").exists(),
            "Copy mode should have placed the RAF in {{shoot}}/RAW/"
        );
        assert!(
            raw_dir.join("DSCF0001.JPG").exists(),
            "Copy mode should have placed the sibling JPG next to the RAW"
        );
        assert!(raw_dir.join("standalone.jpg").exists());

        // AI worker contract: one work item per frame, never per file.
        let needing = db.photos_needing_ai(shoot_id).unwrap();
        assert_eq!(needing.len(), 2, "AI must enqueue once per frame, not per file");

        // --- (7) Reject the paired frame and run layout sync.
        // (Contract c)
        let paired_id = ids
            .iter()
            .copied()
            .find(|id| {
                db.get_photo_by_id(*id)
                    .map(|p| p.filename.eq_ignore_ascii_case("DSCF0001.RAF"))
                    .unwrap_or(false)
            })
            .unwrap();
        db.set_flag(paired_id, "reject").unwrap();

        let report = layout::sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(
            report.errors.is_empty(),
            "layout sync should have zero errors, got {:?}",
            report.errors
        );
        assert!(report.missing.is_empty(), "no files should be missing");

        // Both the RAF and the JPG must now sit in RAW/rejects/. The
        // unrelated lone JPG must be untouched.
        let rejects = raw_dir.join("rejects");
        assert!(
            rejects.join("DSCF0001.RAF").exists(),
            "RAF should have moved to RAW/rejects/"
        );
        assert!(
            rejects.join("DSCF0001.JPG").exists(),
            "sibling JPG should follow the RAF into RAW/rejects/"
        );
        assert!(
            !raw_dir.join("DSCF0001.RAF").exists(),
            "RAF must no longer be at the top level"
        );
        assert!(
            !raw_dir.join("DSCF0001.JPG").exists(),
            "JPG must no longer be at the top level"
        );
        assert!(
            raw_dir.join("standalone.jpg").exists(),
            "lone JPG must not have moved"
        );

        // DB is updated to reflect the new locations.
        let after = db.get_photo_by_id(paired_id).unwrap();
        assert!(
            after.raw_path.contains("rejects"),
            "raw_path should now point at rejects/, got {}",
            after.raw_path
        );
        assert!(
            after
                .sidecar_jpeg_path
                .as_deref()
                .map(|p| p.contains("rejects"))
                .unwrap_or(false),
            "sidecar_jpeg_path should track the move, got {:?}",
            after.sidecar_jpeg_path
        );

        // --- (8) Re-running sync is a no-op. (Contract d)
        let report2 = layout::sync_shoot_layout(&db, shoot_id).unwrap();
        assert!(
            report2.moved.is_empty(),
            "second sync should not move anything, got {:?}",
            report2.moved
        );
        assert!(report2.errors.is_empty());
    }
}
