use crate::ingest::{preview, walker};
use crate::metadata::{exif, orientation};
use base64::Engine;
use image::GenericImageView;
use jpeg_encoder::{ColorType, Encoder as JpegEncoder};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// One entry in the pre-import scan response. Cheap enough to produce
/// per-file that scanning a 200-photo folder completes in seconds —
/// no SHA-256, no copy, just the embedded JPEG plus EXIF metadata we
/// already know how to extract.
#[derive(serde::Serialize)]
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

#[tauri::command]
pub fn scan_folder(source: String) -> Result<Vec<ScanEntry>, String> {
    let src_path = PathBuf::from(&source);
    if !src_path.exists() {
        return Err(format!("Source path does not exist: {}", source));
    }
    let files = walker::walk_source(&src_path);
    // Parallel over files — each scan is independent, disk + decode bound.
    let mut entries: Vec<ScanEntry> = files
        .par_iter()
        .map(|p| scan_one_file(p))
        .collect();
    // Sort by capture time when available, then filename — same order the
    // user would see in Explorer / Finder.
    entries.sort_by(|a, b| {
        a.captured_at
            .cmp(&b.captured_at)
            .then_with(|| a.filename.cmp(&b.filename))
    });
    Ok(entries)
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
