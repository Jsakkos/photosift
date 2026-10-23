//! Maps a past shoot's `Export\` files back to the originals they came from.
//! Read-only: export files are listed and read, never touched.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::Serialize;

use crate::ingest::scan::is_hidden;
use crate::media::exif;

/// Image formats an export can be matched in. Any other file (a clip, a
/// stray document) is listed as unmatched: videos are never indexed.
const EXPORT_EXTS: &[&str] = &["jpg", "jpeg", "tif", "tiff", "png", "heic", "heif"];

/// An indexed original, as the matcher sees it.
#[derive(Debug, Clone)]
pub struct PhotoKey {
    pub id: i64,
    /// The original's file stem (the RAW's, else the JPEG's).
    pub stem: String,
    /// EXIF `DateTimeOriginal` as read at ingest, before any camera offset.
    pub capture_time: Option<NaiveDateTime>,
    pub camera: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExportMatch {
    pub matched: Vec<(PathBuf, i64)>,
    /// Paths relative to the export folder; never silently dropped.
    pub unmatched: Vec<String>,
}

/// Rule 1: an export stem equal to exactly one original's stem
/// (case-insensitive), first as is, then with Lightroom's `-Edit`, `-2` and
/// ` (2)` suffixes peeled off one at a time. Rule 2: EXIF `DateTimeOriginal`
/// and camera equal to exactly one original's. Otherwise, and for every file
/// that is not an image, unmatched. An unreadable folder yields an empty
/// result; the caller checks it exists.
pub fn match_exports(export_dir: &Path, photos: &[PhotoKey]) -> ExportMatch {
    let mut by_stem: HashMap<String, Vec<i64>> = HashMap::new();
    for p in photos {
        by_stem.entry(p.stem.to_lowercase()).or_default().push(p.id);
    }
    let mut out = ExportMatch::default();
    for path in export_files(export_dir) {
        let image = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXPORT_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)));
        let id = image
            .then(|| by_stem_rule(&path, &by_stem).or_else(|| by_exif_rule(&path, photos)))
            .flatten();
        match id {
            Some(id) => out.matched.push((path, id)),
            None => out.unmatched.push(
                path.strip_prefix(export_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned(),
            ),
        }
    }
    out
}

fn by_stem_rule(path: &Path, by_stem: &HashMap<String, Vec<i64>>) -> Option<i64> {
    let stem = path.file_stem()?.to_string_lossy();
    for candidate in stem_candidates(&stem) {
        match by_stem.get(&candidate).map(Vec::as_slice) {
            Some([id]) => return Some(*id),
            // Two originals share the stem (a counter rollover): only EXIF
            // can tell them apart.
            Some(_) => return None,
            None => {}
        }
    }
    None
}

/// The lowercased stem, then each shorter form with one more Lightroom
/// suffix removed (`DSC_1-Edit-2` → `dsc_1-edit` → `dsc_1`).
fn stem_candidates(stem: &str) -> Vec<String> {
    let mut out = vec![stem.to_lowercase()];
    loop {
        let last = out.last().expect("never empty");
        let Some(shorter) = strip_suffix(last) else {
            return out;
        };
        let shorter = shorter.to_string();
        if shorter.is_empty() {
            return out;
        }
        out.push(shorter);
    }
}

fn strip_suffix(s: &str) -> Option<&str> {
    if let Some(rest) = s.strip_suffix("-edit") {
        return Some(rest);
    }
    // " (12)"
    if let Some(inner) = s.strip_suffix(')') {
        if let Some(open) = inner.rfind(" (") {
            let digits = &inner[open + 2..];
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                return Some(&s[..open]);
            }
        }
    }
    // "-12"
    let digits = s.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let head = &s[..s.len() - digits];
        if let Some(rest) = head.strip_suffix('-') {
            return Some(rest);
        }
    }
    None
}

fn by_exif_rule(path: &Path, photos: &[PhotoKey]) -> Option<i64> {
    let bytes = fs::read(path).ok()?;
    let info = exif::read(&bytes, path);
    let (time, camera) = (info.capture_time?, camera_model(info.camera.as_deref()?));
    let mut hits = photos.iter().filter(|p| {
        p.capture_time == Some(time)
            && p.camera.as_deref().map(camera_model) == Some(camera.clone())
    });
    match (hits.next(), hits.next()) {
        (Some(p), None) => Some(p.id),
        _ => None,
    }
}

/// Make and model without the `#serial` that `exif::read` appends: an export
/// preset may strip the body serial while keeping make and model.
fn camera_model(camera: &str) -> String {
    let base = camera.split(" #").next().unwrap_or(camera);
    base.trim().to_lowercase()
}

/// Every file under `dir`, recursively, sorted; hidden files and links are
/// skipped.
fn export_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            // Thumbs.db and dot-files are no exports.
            if meta.file_type().is_symlink() || is_hidden(&entry.file_name(), &meta) {
                continue;
            }
            let path = entry.path();
            if meta.is_dir() {
                pending.push(path);
            } else if meta.is_file() {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightroom_suffixes_peel_one_at_a_time() {
        assert_eq!(
            stem_candidates("DSC_0001-Edit-2"),
            ["dsc_0001-edit-2", "dsc_0001-edit", "dsc_0001"]
        );
        assert_eq!(
            stem_candidates("DSCF0002 (3)"),
            ["dscf0002 (3)", "dscf0002"]
        );
        assert_eq!(stem_candidates("family"), ["family"]);
        assert_eq!(stem_candidates("-12"), ["-12"]);
    }

    #[test]
    fn camera_model_drops_the_serial() {
        assert_eq!(
            camera_model("NIKON CORPORATION NIKON D750 #3012345"),
            "nikon corporation nikon d750"
        );
        assert_eq!(camera_model("FUJIFILM"), "fujifilm");
    }
}
