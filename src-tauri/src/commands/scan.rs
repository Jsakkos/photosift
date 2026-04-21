use crate::ingest::{preview, walker};
use crate::metadata::{exif, orientation};
use base64::Engine;
use image::GenericImageView;
use jpeg_encoder::{ColorType, Encoder as JpegEncoder};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    pub file_size_bytes: u64,
    /// A 200-px longest-edge thumbnail as a data URL (`data:image/jpeg;base64,...`).
    /// `None` when the embedded JPEG couldn't be decoded — the UI should
    /// fall back to a filename-only tile in that case.
    pub thumb_data_url: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub index: usize,
    pub total: usize,
    pub entry: ScanEntry,
}

/// A 200-photo NEF folder will otherwise fan out to `num_cpus` workers, each
/// reading a 50–200 MB file and decoding the embedded JPEG — the RAM footprint
/// was the bottleneck, not the CPU. Four is enough to saturate an SSD on the
/// D750 preview sizes I've tested without piling decodes on top of each other.
const SCAN_PARALLELISM: usize = 4;

#[tauri::command]
pub async fn scan_folder(app: AppHandle, source: String) -> Result<usize, String> {
    // Must run inside spawn_blocking so the synchronous rayon work doesn't
    // hold the IPC worker: emit() queues events to the webview, but they
    // only drain while the command task yields. A blocking sync command
    // would let the scan complete in the background but deliver every
    // `scan-progress` event in one burst after it returned — the UI would
    // see a 30-second frozen panel and then 398 thumbnails at once.
    tauri::async_runtime::spawn_blocking(move || scan_folder_blocking(app, source))
        .await
        .map_err(|e| format!("scan task panicked: {}", e))?
}

fn scan_folder_blocking(app: AppHandle, source: String) -> Result<usize, String> {
    let src_path = PathBuf::from(&source);
    if !src_path.exists() {
        return Err(format!("Source path does not exist: {}", source));
    }
    let files = walker::walk_source(&src_path);
    let total = files.len();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(SCAN_PARALLELISM)
        .thread_name(|i| format!("photosift-scan-{}", i))
        .build()
        .map_err(|e| format!("failed to build scan pool: {}", e))?;

    let counter = AtomicUsize::new(0);
    pool.install(|| {
        files.par_iter().for_each(|p| {
            let entry = scan_one_file(p);
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

    Ok(total)
}

fn scan_one_file(path: &Path) -> ScanEntry {
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let exif_data = exif::extract_exif(path).ok();
    let captured_at = exif_data.as_ref().and_then(|e| e.capture_time.clone());
    let orientation_tag = exif_data.as_ref().and_then(|e| e.orientation);

    let thumb_data_url = build_scan_thumb(path, orientation_tag);
    if thumb_data_url.is_none() {
        log::warn!(
            "scan: no preview thumbnail for {} — embedded JPEG decode failed",
            path.display()
        );
    }

    ScanEntry {
        path: path.to_string_lossy().into_owned(),
        filename,
        captured_at,
        file_size_bytes,
        thumb_data_url,
    }
}

fn build_scan_thumb(path: &Path, orientation_tag: Option<i32>) -> Option<String> {
    let (_bytes, decoded) = preview::extract_and_decode(path).ok()?;
    let img = decoded?;
    let upright = orientation::apply(img, orientation_tag);
    let small = upright.thumbnail(200, 200);
    let (w, h) = small.dimensions();
    let rgb = small.to_rgb8();

    let mut buf: Vec<u8> = Vec::new();
    let encoder = JpegEncoder::new(&mut buf, 72);
    encoder
        .encode(rgb.as_raw(), w as u16, h as u16, ColorType::Rgb)
        .ok()?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&buf);
    Some(format!("data:image/jpeg;base64,{}", encoded))
}
