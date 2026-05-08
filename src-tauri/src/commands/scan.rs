use crate::db::schema::Database;
use crate::ingest::{preview, walker};
use crate::metadata::{exif, orientation};
use base64::Engine;
use image::GenericImageView;
use jpeg_encoder::{ColorType, Encoder as JpegEncoder};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// One entry in the pre-import scan response. Cheap enough to produce
/// per-file that scanning a 200-photo folder completes in seconds —
/// no SHA-256, no copy, just the embedded JPEG plus EXIF metadata we
/// already know how to extract.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanEntry {
    /// Absolute path on disk. The frontend passes this back unchanged
    /// when the user commits the import so we avoid re-walking.
    pub path: String,
    pub filename: String,
    pub captured_at: Option<String>,
    pub camera: Option<String>,
    pub file_size_bytes: u64,
    /// A 200-px longest-edge thumbnail as a data URL (`data:image/jpeg;base64,...`).
    /// `None` when the embedded JPEG couldn't be decoded — the UI should
    /// fall back to a filename-only tile in that case.
    pub thumb_data_url: Option<String>,
    /// True when (camera, filename, file_size) matches an already-imported
    /// photo per the heuristic dedup index. Only populated when the caller
    /// passed `dedup_known: true`. SHA-256 dedup at actual import time is
    /// the source of truth.
    pub already_imported: bool,
    /// EXIF orientation tag (1–8) when present. The date-browser uses this
    /// to render portrait NEFs with the correct 2:3 aspect ratio instead
    /// of falling back to landscape 3:2.
    pub orientation: Option<i32>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub index: usize,
    pub total: usize,
    pub entry: ScanEntry,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanThumbReady {
    pub path: String,
    pub thumb_data_url: Option<String>,
}

/// A 200-photo NEF folder will otherwise fan out to `num_cpus` workers, each
/// reading a 50–200 MB file and decoding the embedded JPEG — the RAM footprint
/// was the bottleneck, not the CPU. Four is enough to saturate an SSD on the
/// D750 preview sizes I've tested without piling decodes on top of each other.
const SCAN_PARALLELISM: usize = 4;

#[tauri::command]
pub async fn scan_folder(
    app: AppHandle,
    source: String,
    // When false (default flow), skip the embedded-JPEG decode and emit
    // entries with `thumb_data_url = None`. Walking + EXIF is cheap; the
    // decode is what took 10+ seconds on a 400-photo NEF folder. The
    // ImportDialog only needs thumbs when the user flips "Select subset"
    // so they can visually pick which frames to drop.
    with_thumbnails: Option<bool>,
    // When true, populate `already_imported` per entry by checking the
    // heuristic dedup index (camera, filename, file_size). Used by the
    // SD-card date browser so leftover history can be hidden by default.
    dedup_known: Option<bool>,
) -> Result<usize, String> {
    let with_thumbnails = with_thumbnails.unwrap_or(false);
    let dedup_known = dedup_known.unwrap_or(false);
    // Must run inside spawn_blocking so the synchronous rayon work doesn't
    // hold the IPC worker: emit() queues events to the webview, but they
    // only drain while the command task yields. A blocking sync command
    // would let the scan complete in the background but deliver every
    // `scan-progress` event in one burst after it returned — the UI would
    // see a 30-second frozen panel and then 398 thumbnails at once.
    tauri::async_runtime::spawn_blocking(move || {
        scan_folder_blocking(app, source, with_thumbnails, dedup_known)
    })
    .await
    .map_err(|e| format!("scan task panicked: {}", e))?
}

fn scan_folder_blocking(
    app: AppHandle,
    source: String,
    with_thumbnails: bool,
    dedup_known: bool,
) -> Result<usize, String> {
    let src_path = PathBuf::from(&source);
    if !src_path.exists() {
        return Err(format!("Source path does not exist: {}", source));
    }
    let files = walker::walk_source(&src_path);
    let total = files.len();

    let known: Arc<HashSet<(String, String, u64)>> = if dedup_known {
        let set = Database::open_global()
            .and_then(|db| db.known_originals())
            .unwrap_or_else(|e| {
                log::warn!("scan: known_originals failed, dedup disabled: {}", e);
                HashSet::new()
            });
        Arc::new(set)
    } else {
        Arc::new(HashSet::new())
    };

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(SCAN_PARALLELISM)
        .thread_name(|i| format!("photosift-scan-{}", i))
        .build()
        .map_err(|e| format!("failed to build scan pool: {}", e))?;

    let counter = AtomicUsize::new(0);
    pool.install(|| {
        files.par_iter().for_each(|p| {
            let entry = scan_one_file(p, with_thumbnails, &known);
            let index = counter.fetch_add(1, Ordering::Relaxed);
            let _ = app.emit(
                "scan-progress",
                ScanProgress {
                    index,
                    total,
                    entry,
                },
            );
        });
    });

    let _ = app.emit("scan-complete", total);

    Ok(total)
}

fn scan_one_file(
    path: &Path,
    with_thumbnails: bool,
    known: &HashSet<(String, String, u64)>,
) -> ScanEntry {
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let exif_data = exif::extract_exif(path).ok();
    let captured_at = exif_data.as_ref().and_then(|e| e.capture_time.clone());
    let camera = exif_data.as_ref().and_then(|e| e.camera_model.clone());
    let orientation_tag = exif_data.as_ref().and_then(|e| e.orientation);

    let already_imported = match camera.as_ref() {
        Some(cam) if !known.is_empty() => known.contains(&(
            cam.to_lowercase(),
            filename.to_lowercase(),
            file_size_bytes,
        )),
        _ => false,
    };

    let thumb_data_url = if with_thumbnails {
        build_scan_thumb(path, orientation_tag)
    } else {
        None
    };

    ScanEntry {
        path: path.to_string_lossy().into_owned(),
        filename,
        captured_at,
        camera,
        file_size_bytes,
        thumb_data_url,
        already_imported,
        orientation: orientation_tag,
    }
}

/// Lazy thumbnail extraction for a caller-specified subset of paths.
/// Used by the SD-card date browser to defer preview decoding until the
/// user expands a day. Emits `scan-thumb-ready` per file.
#[tauri::command]
pub async fn extract_thumbnails_for_paths(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<usize, String> {
    let total = paths.len();
    tauri::async_runtime::spawn_blocking(move || {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(SCAN_PARALLELISM)
            .thread_name(|i| format!("photosift-thumb-{}", i))
            .build()
            .map_err(|e| format!("failed to build thumb pool: {}", e))?;

        pool.install(|| {
            paths.par_iter().for_each(|p| {
                let path = PathBuf::from(p);
                let orientation_tag = exif::extract_exif(&path)
                    .ok()
                    .and_then(|e| e.orientation);
                let thumb = build_scan_thumb(&path, orientation_tag);
                let _ = app.emit(
                    "scan-thumb-ready",
                    ScanThumbReady {
                        path: p.clone(),
                        thumb_data_url: thumb,
                    },
                );
            });
        });

        Ok::<usize, String>(total)
    })
    .await
    .map_err(|e| format!("thumb task panicked: {}", e))?
}

fn build_scan_thumb(path: &Path, orientation_tag: Option<i32>) -> Option<String> {
    // Fast path: read the IFD1 thumbnail (10–20 KB at the start of the
    // file) instead of pulling in the entire 30–50 MB NEF. This is what
    // Capture One / Lightroom / Windows Explorer all do for date-browser
    // tiles. Falls back to the largest-JPEG scan only when IFD1 is
    // absent or its bytes don't decode.
    let decoded = if let Ok(bytes) = exif::extract_ifd1_thumbnail(path) {
        match preview::decode_jpeg_to_image(&bytes) {
            Ok(img) => Some(img),
            Err(e) => {
                log::debug!(
                    "scan_thumb: {} — IFD1 thumbnail bytes failed to decode ({}); falling back",
                    path.display(),
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let decoded = match decoded {
        Some(img) => img,
        None => {
            // Fallback: the original path that walks every embedded JPEG
            // largest-first. Two failure modes — extraction error vs.
            // every candidate failing to decode — get distinct logs.
            match preview::extract_and_decode(path) {
                Ok((_bytes, Some(img))) => img,
                Ok((_bytes, None)) => {
                    log::warn!(
                        "scan_thumb: {} — embedded JPEG candidates all failed to decode",
                        path.display()
                    );
                    return None;
                }
                Err(e) => {
                    log::warn!(
                        "scan_thumb: {} — failed to extract embedded JPEG: {}",
                        path.display(),
                        e
                    );
                    return None;
                }
            }
        }
    };

    let upright = orientation::apply(decoded, orientation_tag);
    let small = upright.thumbnail(200, 200);
    let (w, h) = small.dimensions();
    let rgb = small.to_rgb8();

    let mut buf: Vec<u8> = Vec::new();
    let encoder = JpegEncoder::new(&mut buf, 72);
    if let Err(e) = encoder.encode(rgb.as_raw(), w as u16, h as u16, ColorType::Rgb) {
        log::warn!(
            "scan_thumb: {} — JPEG re-encode failed at {}x{}: {}",
            path.display(),
            w,
            h,
            e
        );
        return None;
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&buf);
    Some(format!("data:image/jpeg;base64,{}", encoded))
}
