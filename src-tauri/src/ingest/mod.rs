pub mod clustering;
pub mod copy;
pub mod hashing;
pub mod phash;
pub mod preview;
pub mod progress;
pub mod thumbnail;
pub mod walker;

use crate::db::schema::{Database, PhotoInsert};
use crate::metadata::{exif, orientation, xmp};
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
    let mut files = walker::walk_source(&source);
    // Pre-import selection filter: when the user cherry-picked from the
    // scan dialog we intersect on absolute paths. Preserves walker order
    // so counters and progress events still make sense.
    if let Some(selected) = selected_paths {
        use std::collections::HashSet;
        let wanted: HashSet<PathBuf> = selected.into_iter().collect();
        files.retain(|p| wanted.contains(p));
    }
    if files.is_empty() {
        return Err("No supported image files found in source directory".into());
    }
    let total = files.len();

    // Phase 2: Probe first file for EXIF date to derive YYYY-MM
    let yyyy_mm = derive_yyyy_mm(&files[0]);

    // Phase 3: Create shoot row and directories.
    // Copy mode derives a canonical folder under the user's library root;
    // in-place mode registers files where they are and records the source
    // folder as the effective dest_path.
    let configured_lib_root: Option<PathBuf> = {
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        db_guard.get_settings().ok().and_then(|s| s.library_root.map(PathBuf::from))
    };
    let shoot_dir = match import_mode {
        ImportMode::Copy => {
            let lib_root = configured_lib_root.unwrap_or_else(copy::library_root);
            copy::shoot_folder(&lib_root, &yyyy_mm, &slug)
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

    let results: Vec<ProcessedFile> = files
        .par_iter()
        .map(|src_path| {
            if cancel.load(Ordering::Relaxed) {
                return ProcessedFile::Skipped;
            }

            let result = process_one_file(
                src_path,
                &shoot_dir,
                &previews_dir,
                &thumbs_dir,
                &db,
                import_mode,
            );

            let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
            let fname = src_path
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
    src_path: &Path,
    shoot_dir: &Path,
    previews_dir: &Path,
    thumbs_dir: &Path,
    db: &Mutex<Database>,
    import_mode: ImportMode,
) -> ProcessedFile {
    let t_start = Instant::now();

    // 1. SHA-256
    let t_sha = Instant::now();
    let content_hash = match hashing::sha256_stream(src_path) {
        Ok(h) => h,
        Err(e) => {
            log::error!("SHA-256 failed for {:?}: {}", src_path, e);
            return ProcessedFile::Skipped;
        }
    };
    let sha_ms = t_sha.elapsed().as_secs_f64() * 1000.0;

    // 2. Dedup check
    if let Ok(guard) = db.lock() {
        if let Ok(Some(_)) = guard.photo_exists_by_hash(&content_hash) {
            return ProcessedFile::Skipped;
        }
    }

    let filename = src_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // 3. EXIF
    let t_exif = Instant::now();
    let exif_data = exif::extract_exif(src_path).ok();
    let exif_ms = t_exif.elapsed().as_secs_f64() * 1000.0;

    // 3b. Look for an existing XMP sidecar next to the source file. If present,
    // prefer its rating/label over EXIF (XMP is the more recently-written
    // metadata when the user has culled in another tool like DxO or C1).
    let sidecar_rating = xmp::read_rating(src_path);
    let sidecar_flag = xmp::read_flag_from_label(src_path);
    let initial_star_rating = sidecar_rating
        .or_else(|| exif_data.as_ref().and_then(|e| e.rating));
    let initial_flag = sidecar_flag;

    // 4. In copy mode, copy file into the canonical location. In in-place
    //    mode, raw_path is simply the source path (the file is left where it is).
    let t_copy = Instant::now();
    let raw_path = match import_mode {
        ImportMode::Copy => {
            let dest = copy::plan_dest(shoot_dir, &filename);
            match copy::copy_file(src_path, &dest) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("Copy failed for {:?}: {}", src_path, e);
                    return ProcessedFile::Skipped;
                }
            }
        }
        ImportMode::InPlace => src_path.to_path_buf(),
    };
    let copy_ms = t_copy.elapsed().as_secs_f64() * 1000.0;

    // 5-8. Extract primary JPEG bytes (kept for the disk preview) and
    // also try to decode any embedded JPEG candidate (NEFs usually have
    // a smaller standard-baseline preview that decodes even when the
    // full-resolution JPEG uses arithmetic coding or 12-bit precision).
    let t_preview = Instant::now();
    let (preview_bytes, decoded) = match preview::extract_and_decode(&raw_path) {
        Ok(v) => v,
        Err(e) => {
            log::error!("JPEG extraction failed for {:?}: {}", raw_path, e);
            return ProcessedFile::Skipped;
        }
    };
    let preview_ms = t_preview.elapsed().as_secs_f64() * 1000.0;

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
        "ingest::process_one_file {} total={:.1}ms (sha={:.1} exif={:.1} copy={:.1} preview={:.1} rotate={:.1} thumb+phash={:.1})",
        filename,
        total_ms,
        sha_ms,
        exif_ms,
        copy_ms,
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
        initial_flag,
        initial_star_rating,
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
