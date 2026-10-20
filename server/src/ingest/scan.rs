//! Card/folder scan: walks a source, pairs RAW+JPEG, reads just enough EXIF to
//! date each frame, and proposes events. Never writes to the source.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File, Metadata};
use std::hash::{BuildHasher, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Local, NaiveDateTime, Timelike};
use serde::Serialize;

use super::events::{split_into_events, ProposedEvent};
use crate::config::Settings;
use crate::db::{photos::TimeSource, DbPool};
use crate::media::exif::{self, ExifInfo};

/// Enough for the EXIF block of a JPEG or the camera JPEG at the head of a
/// RAF; scanning a 64 GB card must not read 64 GB.
const PREFIX_BYTES: u64 = 512 * 1024;

const RAW_EXTS: &[&str] = &["raf", "nef"];
const JPEG_EXTS: &[&str] = &["jpg", "jpeg"];
const VIDEO_EXTS: &[&str] = &["mov", "mp4"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Photo,
    Video,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanItem {
    /// Position in `CardScan::items`; events refer to items by this.
    pub idx: usize,
    /// The RAW when there is one, else the JPEG or video.
    pub primary: PathBuf,
    pub sibling_jpeg: Option<PathBuf>,
    pub kind: ItemKind,
    /// `"Unknown"` when the file carries no Make/Model.
    pub camera: String,
    /// Naive camera-local time; no timezone is known or assumed.
    pub capture_time: NaiveDateTime,
    pub time_source: TimeSource,
    /// Everything that will be copied for this item: primary plus sibling JPEG.
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CameraRange {
    pub camera: String,
    pub count: usize,
    pub first: NaiveDateTime,
    pub last: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardScan {
    /// Opaque id under which the import step finds this scan again.
    pub scan_id: String,
    pub source: PathBuf,
    /// Chronological by `capture_time`.
    pub items: Vec<ScanItem>,
    pub cameras: Vec<CameraRange>,
    /// Items whose primary file already matches a `files` row by name and
    /// size. A cheap hint only; the SHA check at import time is authoritative.
    pub already_imported: usize,
    /// Proposed with zero camera offsets; the user adjusts offsets afterwards.
    pub events: Vec<ProposedEvent>,
    /// Directories or entries the walk could not read. Surfaced so a card that
    /// is only partly readable never looks fully scanned.
    pub warnings: Vec<String>,
}

/// Blocking: call from `spawn_blocking` inside async handlers.
pub fn scan(source: &Path, db: &DbPool, settings: &Settings) -> anyhow::Result<CardScan> {
    let (files, warnings) = walk(source)?;

    let mut items = Vec::new();
    for paired in pair(files) {
        items.push(build_item(paired)?);
    }
    items.sort_by(|a, b| {
        a.capture_time
            .cmp(&b.capture_time)
            .then_with(|| a.primary.cmp(&b.primary))
    });
    for (idx, item) in items.iter_mut().enumerate() {
        item.idx = idx;
    }

    let events = split_into_events(
        &items,
        &HashMap::new(),
        settings.event_gap_minutes,
        settings.everyday_max_photos,
    );
    Ok(CardScan {
        scan_id: new_id(),
        source: source.to_path_buf(),
        cameras: camera_ranges(&items),
        already_imported: count_already_imported(db, &items)?,
        items,
        events,
        warnings,
    })
}

fn camera_ranges(items: &[ScanItem]) -> Vec<CameraRange> {
    let mut by_camera: BTreeMap<&str, CameraRange> = BTreeMap::new();
    for it in items {
        by_camera
            .entry(&it.camera)
            .and_modify(|r| {
                r.count += 1;
                r.first = r.first.min(it.capture_time);
                r.last = r.last.max(it.capture_time);
            })
            .or_insert_with(|| CameraRange {
                camera: it.camera.clone(),
                count: 1,
                first: it.capture_time,
                last: it.capture_time,
            });
    }
    by_camera.into_values().collect()
}

fn count_already_imported(db: &DbPool, items: &[ScanItem]) -> anyhow::Result<usize> {
    db.with(|c| {
        let mut stmt = c.prepare("SELECT 1 FROM files WHERE name = ?1 AND size = ?2 LIMIT 1")?;
        let mut n = 0;
        for it in items {
            let Some(name) = it.primary.file_name() else {
                continue;
            };
            // The primary alone: a RAW's sibling JPEG would double-count the frame.
            let size = fs::metadata(&it.primary).map(|m| m.len()).unwrap_or(0) as i64;
            if stmt.exists(rusqlite::params![name.to_string_lossy(), size])? {
                n += 1;
            }
        }
        Ok(n)
    })
}

// --- id -------------------------------------------------------------------

/// Opaque 32-hex-char id for scans and import jobs.
pub(crate) fn new_id() -> String {
    // RandomState is seeded from the OS per process and varies per instance,
    // which is plenty for an in-memory key without a uuid dependency.
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    let a = h.finish();
    let b = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!("{a:016x}{b:016x}")
}

// --- walking --------------------------------------------------------------

fn has_ext(path: &Path, set: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| set.iter().any(|s| e.eq_ignore_ascii_case(s)))
}

fn is_hidden(name: &std::ffi::OsStr, meta: &Metadata) -> bool {
    // Dot-files cover macOS "._" resource forks; the attribute check covers
    // "System Volume Information" and "$RECYCLE.BIN" on a FAT/exFAT card.
    if name.to_string_lossy().starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN_OR_SYSTEM: u32 = 0x2 | 0x4;
        meta.file_attributes() & HIDDEN_OR_SYSTEM != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

/// Every supported file under `root`, sorted so pairing and ordering are
/// deterministic, plus a warning for each directory or entry that could not be
/// read. An unreadable root is an error.
fn walk(root: &Path) -> anyhow::Result<(Vec<PathBuf>, Vec<String>)> {
    walk_with(root, &|p| fs::read_dir(p))
}

/// `list` is injectable because a permission-denied directory is awkward to
/// stage portably in tests.
fn walk_with(
    root: &Path,
    list: &dyn Fn(&Path) -> std::io::Result<fs::ReadDir>,
) -> anyhow::Result<(Vec<PathBuf>, Vec<String>)> {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = match list(&dir) {
            Ok(e) => e,
            Err(e) if dir == root => {
                return Err(e).with_context(|| format!("reading {}", root.display()))
            }
            Err(e) => {
                warnings.push(format!("cannot read directory {}: {e}", dir.display()));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    warnings.push(format!("cannot read an entry in {}: {e}", dir.display()));
                    continue;
                }
            };
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(e) => {
                    warnings.push(format!("cannot stat {}: {e}", entry.path().display()));
                    continue;
                }
            };
            // Never follow links: a junction on a card could loop or leave it.
            if meta.file_type().is_symlink() || is_hidden(&entry.file_name(), &meta) {
                continue;
            }
            let path = entry.path();
            if meta.is_dir() {
                pending.push(path);
            } else if meta.is_file()
                && (has_ext(&path, RAW_EXTS)
                    || has_ext(&path, JPEG_EXTS)
                    || has_ext(&path, VIDEO_EXTS))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok((found, warnings))
}

// --- pairing --------------------------------------------------------------

/// A frame as the camera wrote it: one RAW plus optional JPEG, a lone JPEG, or
/// a video.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Paired {
    primary: PathBuf,
    sibling_jpeg: Option<PathBuf>,
    kind: ItemKind,
}

fn pair_key(path: &Path) -> Option<(PathBuf, String)> {
    let parent = path.parent()?.to_path_buf();
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    Some((parent, stem))
}

/// Ported from the Tauri app's `ingest::pairing`. Pairing rule: same parent
/// directory and same file stem (case-insensitive). A JPEG with several RAW
/// partners attaches to the first only. Videos never pair. When one RAW has
/// several same-stem JPEGs (A.JPG and A.JPEG) the first attaches and the rest
/// become lone JPEG items, so every media file on the card is still copied.
fn pair(files: Vec<PathBuf>) -> Vec<Paired> {
    // Index up front so a JPEG that sorts before its RAW is still recognised
    // as a sibling rather than emitted as a lone photo.
    let mut jpeg_index: HashMap<(PathBuf, String), usize> = HashMap::new();
    let mut raw_keys: HashSet<(PathBuf, String)> = HashSet::new();
    for (i, path) in files.iter().enumerate() {
        if let Some(key) = pair_key(path) {
            if has_ext(path, RAW_EXTS) {
                raw_keys.insert(key);
            } else if has_ext(path, JPEG_EXTS) {
                jpeg_index.entry(key).or_insert(i);
            }
        }
    }
    // Only JPEGs with a RAW partner can be siblings; remember which one wins.
    let attachable: HashSet<usize> = jpeg_index
        .iter()
        .filter(|(key, _)| raw_keys.contains(*key))
        .map(|(_, &i)| i)
        .collect();

    let mut items = Vec::with_capacity(files.len());
    for (i, path) in files.iter().enumerate() {
        if has_ext(path, RAW_EXTS) {
            let sibling = pair_key(path)
                .and_then(|key| jpeg_index.remove(&key))
                .map(|j| files[j].clone());
            items.push(Paired {
                primary: path.clone(),
                sibling_jpeg: sibling,
                kind: ItemKind::Photo,
            });
        } else if has_ext(path, JPEG_EXTS) {
            // The chosen sibling is emitted with its RAW; any other JPEG is a photo.
            if !attachable.contains(&i) {
                items.push(Paired {
                    primary: path.clone(),
                    sibling_jpeg: None,
                    kind: ItemKind::Photo,
                });
            }
        } else {
            items.push(Paired {
                primary: path.clone(),
                sibling_jpeg: None,
                kind: ItemKind::Video,
            });
        }
    }
    items
}

// --- per-item metadata ----------------------------------------------------

fn build_item(p: Paired) -> anyhow::Result<ScanItem> {
    let meta = fs::metadata(&p.primary).with_context(|| format!("stat {}", p.primary.display()))?;
    let mut bytes = meta.len();
    if let Some(j) = &p.sibling_jpeg {
        bytes += fs::metadata(j).map(|m| m.len()).unwrap_or(0);
    }

    let info = match p.kind {
        ItemKind::Photo => read_exif(&p.primary, meta.len()),
        ItemKind::Video => ExifInfo::default(),
    };
    let (capture_time, time_source) = match info.capture_time {
        Some(t) => (t, TimeSource::Exif),
        None => (mtime_local(&meta, &p.primary)?, TimeSource::Mtime),
    };
    Ok(ScanItem {
        idx: 0,
        primary: p.primary,
        sibling_jpeg: p.sibling_jpeg,
        kind: p.kind,
        camera: info.camera.unwrap_or_else(|| "Unknown".to_string()),
        capture_time,
        time_source,
        bytes,
    })
}

/// Reads only the file head first. kamadak-exif is all-or-nothing, so a RAW
/// whose EXIF lies past the prefix fails there; one full read then settles it
/// before the caller falls back to mtime.
fn read_exif(path: &Path, size: u64) -> ExifInfo {
    let head = read_prefix(path)
        .map(|b| exif::read(&b, path))
        .unwrap_or_default();
    if head.capture_time.is_some() || size <= PREFIX_BYTES {
        return head;
    }
    match fs::read(path) {
        Ok(all) => {
            let full = exif::read(&all, path);
            if full.capture_time.is_some() {
                full
            } else {
                head
            }
        }
        Err(_) => head,
    }
}

fn read_prefix(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    File::open(path)?.take(PREFIX_BYTES).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Whole seconds, to match EXIF precision and the stored datetime format.
fn mtime_local(meta: &Metadata, path: &Path) -> anyhow::Result<NaiveDateTime> {
    let modified = meta
        .modified()
        .with_context(|| format!("mtime of {}", path.display()))?;
    Ok(DateTime::<Local>::from(modified)
        .naive_local()
        .with_nanosecond(0)
        .expect("0 ns is always valid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_subdirectory_is_reported_not_skipped_silently() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("DCIM").join("bad");
        fs::create_dir_all(&bad).unwrap();
        fs::write(dir.path().join("DCIM").join("ok.jpg"), b"x").unwrap();
        fs::write(bad.join("lost.jpg"), b"x").unwrap();

        // Permission denial is awkward to stage portably, so the directory
        // lister is injected and fails for one directory.
        let list = |p: &Path| {
            if p == bad {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "denied",
                ))
            } else {
                fs::read_dir(p)
            }
        };
        let (files, warnings) = walk_with(dir.path(), &list).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("bad"), "{warnings:?}");
        assert!(warnings[0].contains("denied"), "{warnings:?}");
    }

    #[test]
    fn unreadable_root_is_still_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let list = |_: &Path| Err(std::io::Error::other("gone"));
        assert!(walk_with(dir.path(), &list).is_err());
    }
}
