//! Pair RAW + sibling-JPEG files at import time.
//!
//! Cameras shooting in "RAW+JPEG" mode write two files per frame: the RAW
//! and a JPEG with the same basename in the same directory (e.g.
//! `DSCF0123.RAF` + `DSCF0123.JPG`). PhotoSift treats the JPEG as a shadow
//! of the RAW — it follows the RAW through layout moves but never gets
//! its own photo row. This keeps the cull views one-row-per-frame and
//! prevents Route/Export from accidentally picking up the JPEG.
//!
//! The walker emits a flat list; this module groups it.

use crate::pipeline::embedded;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportItem {
    /// RAW with a same-stem JPEG alongside it. The JPEG follows the RAW
    /// through layout moves and feeds the preview/thumb pipeline.
    RawWithSibling { raw: PathBuf, jpeg: PathBuf },
    /// Plain RAW, no sibling JPEG.
    RawOnly { raw: PathBuf },
    /// Standalone JPEG with no RAW partner. Becomes a first-class photo
    /// just like before this feature.
    JpegOnly { jpeg: PathBuf },
}

impl ImportItem {
    /// The path that anchors this item. For paired RAW+JPEG and RawOnly
    /// it's the RAW; for JpegOnly it's the JPEG. This is what the
    /// `selected_paths` filter in run_import matches against.
    pub fn primary_path(&self) -> &PathBuf {
        match self {
            ImportItem::RawWithSibling { raw, .. } => raw,
            ImportItem::RawOnly { raw } => raw,
            ImportItem::JpegOnly { jpeg } => jpeg,
        }
    }
}

/// Group a flat file list into RAW+JPEG pairs, lone RAWs, and lone JPEGs.
///
/// Pairing rule: same parent directory, same `file_stem()` (case-insensitive).
/// Order in the result follows the RAW for paired items and the file
/// itself for unpaired items.
///
/// A JPEG that has multiple RAW partners (theoretically possible if a
/// folder somehow has both `IMG.NEF` and `IMG.RAF` next to a single
/// `IMG.JPG`) attaches to the first RAW only.
pub fn pair(files: Vec<PathBuf>) -> Vec<ImportItem> {
    // Pass 1: index every JPEG and collect the set of (parent, stem)
    // keys that have at least one RAW so we can recognize "this JPEG
    // has a RAW partner somewhere in the input" no matter the order.
    let mut jpeg_index: HashMap<(PathBuf, String), usize> = HashMap::new();
    let mut raw_keys: HashSet<(PathBuf, String)> = HashSet::new();
    for (i, path) in files.iter().enumerate() {
        if let Some(key) = pair_key(path) {
            if embedded::is_raw_file(path) {
                raw_keys.insert(key);
            } else if is_jpeg(path) {
                jpeg_index.entry(key).or_insert(i);
            }
        }
    }

    let mut consumed = vec![false; files.len()];
    let mut items: Vec<ImportItem> = Vec::with_capacity(files.len());

    for (i, path) in files.iter().enumerate() {
        if consumed[i] {
            continue;
        }
        if embedded::is_raw_file(path) {
            let mate = pair_key(path)
                .and_then(|key| jpeg_index.remove(&key))
                .filter(|&j| !consumed[j]);
            match mate {
                Some(j) => {
                    consumed[j] = true;
                    items.push(ImportItem::RawWithSibling {
                        raw: path.clone(),
                        jpeg: files[j].clone(),
                    });
                }
                None => items.push(ImportItem::RawOnly { raw: path.clone() }),
            }
        } else if is_jpeg(path) {
            // If a RAW partner exists anywhere in the input, this JPEG
            // is its sibling — skip and let the RAW iteration emit the
            // paired ImportItem. Without this, a JPEG that walks before
            // its RAW (extension sort can do that) would slip through
            // as a JpegOnly photo.
            let has_raw_partner = pair_key(path)
                .map(|key| raw_keys.contains(&key))
                .unwrap_or(false);
            if !has_raw_partner {
                items.push(ImportItem::JpegOnly { jpeg: path.clone() });
            }
        } else {
            // Other supported types (tif/tiff) keep today's behavior:
            // no pairing, treated like a JPEG-only standalone import.
            items.push(ImportItem::JpegOnly { jpeg: path.clone() });
        }
    }

    items
}

/// Look for a same-stem JPEG sibling next to a RAW path. Mirrors the
/// rule used by `pair()` (same parent dir, case-insensitive stem) so
/// that callers operating on a single path — like the lazy thumbnail
/// extractor or scan retries — see the same pairings the bulk pass does.
///
/// Returns the first match in `.JPG`, `.jpg`, `.JPEG`, `.jpeg` order;
/// real-world cameras only ever produce one. `None` if the input isn't
/// a RAW file or if no sibling exists.
pub fn find_sibling_jpeg(raw: &Path) -> Option<PathBuf> {
    if !embedded::is_raw_file(raw) {
        return None;
    }
    let parent = raw.parent()?;
    let stem = raw.file_stem()?.to_str()?;
    for ext in ["JPG", "jpg", "JPEG", "jpeg"] {
        let candidate = parent.join(format!("{}.{}", stem, ext));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn pair_key(path: &std::path::Path) -> Option<(PathBuf, String)> {
    let parent = path.parent()?.to_path_buf();
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    Some((parent, stem))
}

fn is_jpeg(path: &std::path::Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn pairs_nef_and_jpg_in_same_dir() {
        let items = pair(vec![p("/cam/DSC_0001.NEF"), p("/cam/DSC_0001.JPG")]);
        assert_eq!(items.len(), 1);
        match &items[0] {
            ImportItem::RawWithSibling { raw, jpeg } => {
                assert_eq!(raw, &p("/cam/DSC_0001.NEF"));
                assert_eq!(jpeg, &p("/cam/DSC_0001.JPG"));
            }
            other => panic!("expected RawWithSibling, got {:?}", other),
        }
    }

    #[test]
    fn pairs_raf_and_jpg() {
        let items = pair(vec![p("/cam/DSCF0123.RAF"), p("/cam/DSCF0123.JPG")]);
        assert!(matches!(items[0], ImportItem::RawWithSibling { .. }));
    }

    #[test]
    fn pairing_is_case_insensitive_on_stem() {
        let items = pair(vec![p("/cam/IMG_42.NEF"), p("/cam/img_42.jpg")]);
        assert!(matches!(items[0], ImportItem::RawWithSibling { .. }));
    }

    #[test]
    fn jpeg_can_arrive_before_raw_in_walker_order() {
        // Walker sorts file_name DESCENDING, so a `.JPG` can appear before
        // its `.NEF` partner when the extensions sort that way.
        let items = pair(vec![p("/cam/DSC_0001.JPG"), p("/cam/DSC_0001.NEF")]);
        assert_eq!(items.len(), 1);
        match &items[0] {
            ImportItem::RawWithSibling { raw, jpeg } => {
                assert_eq!(raw, &p("/cam/DSC_0001.NEF"));
                assert_eq!(jpeg, &p("/cam/DSC_0001.JPG"));
            }
            other => panic!("expected RawWithSibling, got {:?}", other),
        }
    }

    #[test]
    fn lone_raw_stays_raw_only() {
        let items = pair(vec![p("/cam/DSC_0002.NEF")]);
        assert!(matches!(items[0], ImportItem::RawOnly { .. }));
    }

    #[test]
    fn lone_jpeg_stays_jpeg_only() {
        let items = pair(vec![p("/cam/holiday.jpg")]);
        assert!(matches!(items[0], ImportItem::JpegOnly { .. }));
    }

    #[test]
    fn different_dirs_do_not_pair() {
        let items = pair(vec![p("/cam/a/IMG.NEF"), p("/cam/b/IMG.JPG")]);
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], ImportItem::RawOnly { .. }));
        assert!(matches!(items[1], ImportItem::JpegOnly { .. }));
    }

    #[test]
    fn multiple_pairs_in_same_dir() {
        let items = pair(vec![
            p("/cam/A.NEF"),
            p("/cam/A.JPG"),
            p("/cam/B.NEF"),
            p("/cam/B.JPG"),
            p("/cam/C.NEF"), // lone
            p("/cam/D.JPG"), // lone
        ]);
        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], ImportItem::RawWithSibling { .. }));
        assert!(matches!(items[1], ImportItem::RawWithSibling { .. }));
        assert!(matches!(items[2], ImportItem::RawOnly { .. }));
        assert!(matches!(items[3], ImportItem::JpegOnly { .. }));
    }

    #[test]
    fn primary_path_returns_raw_for_paired_items() {
        let items = pair(vec![p("/cam/X.NEF"), p("/cam/X.JPG")]);
        assert_eq!(items[0].primary_path(), &p("/cam/X.NEF"));
    }

    #[test]
    fn find_sibling_jpeg_finds_uppercase_jpg_next_to_raf() {
        let dir = tempfile::TempDir::new().unwrap();
        let raf = dir.path().join("DSCF0001.RAF");
        let jpg = dir.path().join("DSCF0001.JPG");
        std::fs::write(&raf, b"FUJIFILMCCD-RAW \x00").unwrap();
        std::fs::write(&jpg, b"\xff\xd8").unwrap();

        let found = find_sibling_jpeg(&raf);
        assert_eq!(found.as_deref(), Some(jpg.as_path()));
    }

    #[test]
    fn find_sibling_jpeg_finds_lowercase_jpg_next_to_nef() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw = dir.path().join("IMG_001.NEF");
        let jpg = dir.path().join("IMG_001.jpg");
        std::fs::write(&raw, b"II*\x00").unwrap();
        std::fs::write(&jpg, b"\xff\xd8").unwrap();

        // On case-insensitive filesystems (Windows, default macOS) the
        // helper may return the candidate it tried first (`.JPG`)
        // because `Path::exists()` is case-insensitive there. Both
        // paths open the same file, so all we assert is that a sibling
        // was found and points at the on-disk pair.
        let found = find_sibling_jpeg(&raw).expect("expected a sibling");
        assert!(found.exists(), "returned path must exist on disk");
        assert_eq!(found.parent(), Some(dir.path()));
        assert_eq!(
            found
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase),
            Some("img_001".to_string()),
        );
    }

    #[test]
    fn find_sibling_jpeg_returns_none_when_no_partner() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw = dir.path().join("solo.NEF");
        std::fs::write(&raw, b"II*\x00").unwrap();

        assert!(find_sibling_jpeg(&raw).is_none());
    }

    #[test]
    fn find_sibling_jpeg_returns_none_for_non_raw_input() {
        let dir = tempfile::TempDir::new().unwrap();
        let jpg = dir.path().join("anything.jpg");
        std::fs::write(&jpg, b"\xff\xd8").unwrap();

        assert!(find_sibling_jpeg(&jpg).is_none());
    }

    /// Real-disk integration test mirroring the user's reported folder
    /// shape: 3 paired RAF+JPG frames (the RAW+JPEG mode case), 1 lone
    /// RAF (e.g., a frame where the JPG was deleted), 1 lone JPG. The
    /// walker reads them off disk in name-descending order; pair()
    /// must collapse the paired ones into 5 total ImportItems, not 8.
    #[test]
    fn walker_plus_pair_collapses_paired_files_in_real_dir() {
        use crate::ingest::walker;

        let dir = tempfile::TempDir::new().unwrap();
        let writes: &[(&str, &[u8])] = &[
            ("DSCF0001.RAF", b"FUJIFILMCCD-RAW \0"),
            ("DSCF0001.JPG", b"\xff\xd8\xff\xd9"),
            ("DSCF0002.RAF", b"FUJIFILMCCD-RAW \0"),
            ("DSCF0002.JPG", b"\xff\xd8\xff\xd9"),
            ("DSCF0003.RAF", b"FUJIFILMCCD-RAW \0"),
            ("DSCF0003.JPG", b"\xff\xd8\xff\xd9"),
            ("DSCF0004.RAF", b"FUJIFILMCCD-RAW \0"), // lone RAF
            ("standalone.jpg", b"\xff\xd8\xff\xd9"), // lone JPG
        ];
        for (name, body) in writes {
            std::fs::write(dir.path().join(name), body).unwrap();
        }

        let files = walker::walk_source(dir.path());
        assert_eq!(
            files.len(),
            8,
            "walker must surface every supported image; pairing happens later"
        );

        let items = pair(files);
        assert_eq!(
            items.len(),
            5,
            "3 paired frames + 1 lone RAF + 1 lone JPG = 5 ImportItems"
        );

        let paired = items
            .iter()
            .filter(|i| matches!(i, ImportItem::RawWithSibling { .. }))
            .count();
        let raw_only = items
            .iter()
            .filter(|i| matches!(i, ImportItem::RawOnly { .. }))
            .count();
        let jpeg_only = items
            .iter()
            .filter(|i| matches!(i, ImportItem::JpegOnly { .. }))
            .count();
        assert_eq!(paired, 3);
        assert_eq!(raw_only, 1);
        assert_eq!(jpeg_only, 1);
    }

    /// `start_import`'s selection filter intersects ImportItem
    /// primary_paths against the user's selection. Paired items'
    /// primary_path is the RAW, so the frontend (which sees ScanEntry
    /// with `path = RAW`) sends RAW paths and they match. This test
    /// pins that contract: pair() then filter-by-RAW-paths gives back
    /// the original paired items.
    #[test]
    fn selection_filter_by_raw_paths_keeps_paired_items() {
        use std::collections::HashSet;

        let mut items = pair(vec![
            p("/cam/A.RAF"),
            p("/cam/A.JPG"),
            p("/cam/B.RAF"),
            p("/cam/B.JPG"),
            p("/cam/C.RAF"),
        ]);
        // User selected the first paired frame and the lone RAF.
        let wanted: HashSet<PathBuf> = [p("/cam/A.RAF"), p("/cam/C.RAF")].into_iter().collect();

        items.retain(|item| wanted.contains(item.primary_path()));

        assert_eq!(items.len(), 2);
        match &items[0] {
            ImportItem::RawWithSibling { raw, jpeg } => {
                assert_eq!(raw, &p("/cam/A.RAF"));
                assert_eq!(jpeg, &p("/cam/A.JPG"));
            }
            other => panic!("expected RawWithSibling, got {:?}", other),
        }
        assert!(matches!(items[1], ImportItem::RawOnly { .. }));
    }
}
