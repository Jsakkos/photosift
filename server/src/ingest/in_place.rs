//! Opening a folder that already holds a shoot (one PhotoSift did not create,
//! e.g. one Lightroom imported) as one shoot, indexed in place: rows point at
//! the files where they are (`ImportMode::IndexInPlace`), so nothing is
//! copied and no folder is created. The app's "open in place" and the eval
//! harness share it.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf, Prefix};

use anyhow::{anyhow, bail};
use rusqlite::OptionalExtension;

use super::events::EventKind;
use super::job::{self, EventSpec, ImportRequest};
use super::scan::{self, CardScan, ItemKind, ScanOptions};
use crate::config::Settings;
use crate::db::DbPool;

/// Never indexed as originals: Lightroom's exports (edits made from the
/// originals, which would be duplicates; for the eval they are the answers).
pub const EXCLUDE_DIRS: &[&str] = &["Export"];

const SCAN_OPTIONS: ScanOptions = ScanOptions {
    exclude_dirs: EXCLUDE_DIRS,
};

/// `folder` resolved (`..`, junctions, letter case), required to sit
/// strictly inside the library root and returned under the root as
/// `library_root` spells it. An in-place shoot gets sidecars written beside
/// its files, so only folders of the user's own library qualify, and never
/// the whole library as one shoot. Touches the disk; check that `folder` is
/// a local path first.
pub fn resolve_in_library(folder: &Path, library_root: &Path) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(library_root)
        .map(without_verbatim)
        .map_err(|e| {
            format!(
                "the library folder {} is not available: {e}",
                library_root.display()
            )
        })?;
    let resolved = std::fs::canonicalize(folder)
        .map(without_verbatim)
        .map_err(|e| format!("{}: {e}", folder.display()))?;
    if !resolved.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    match resolved.strip_prefix(&root) {
        // Spelled from the settings' root, like the folders imports create,
        // so a folder PhotoSift imported into is found as that shoot even
        // when the root's spelling differs from its resolved one (8.3 names).
        Ok(rel) if !rel.as_os_str().is_empty() => Ok(library_root.join(rel)),
        _ => Err(format!(
            "only a folder inside the library ({}) can be opened in place",
            library_root.display()
        )),
    }
}

/// `\\?\E:\x` as `E:\x`, the spelling settings and stored shoot folders use.
fn without_verbatim(path: PathBuf) -> PathBuf {
    let mut parts = path.components();
    if let Some(Component::Prefix(p)) = parts.next() {
        if let Prefix::VerbatimDisk(letter) = p.kind() {
            let mut plain = PathBuf::from(format!("{}:\\", char::from(letter)));
            plain.extend(parts.filter(|c| !matches!(c, Component::RootDir)));
            return plain;
        }
    }
    path
}

/// The shoot recorded for `folder`, if any. Case-insensitive, as Windows
/// paths are, so a shoot PhotoSift imported into that folder is found too.
pub fn shoot_for_folder(db: &DbPool, folder: &Path) -> anyhow::Result<Option<i64>> {
    let folder = folder.to_string_lossy();
    db.with(|c| {
        c.query_row(
            "SELECT id FROM shoots WHERE folder = ?1 COLLATE NOCASE ORDER BY id LIMIT 1",
            [folder.as_ref()],
            |r| r.get(0),
        )
        .optional()
    })
}

/// Scans `folder` (not `Export\`) and builds the request that indexes its
/// photos, not its videos, as one named shoot called after the folder; the
/// shoot's folder is `folder` as given. Blocking.
pub fn plan(
    folder: &Path,
    db: &DbPool,
    settings: &Settings,
) -> anyhow::Result<(CardScan, ImportRequest)> {
    let scan = scan::scan_with_options(folder, db, settings, &SCAN_OPTIONS)?;
    let item_idxs: Vec<usize> = scan
        .items
        .iter()
        .filter(|it| it.kind == ItemKind::Photo)
        .map(|it| it.idx)
        .collect();
    if item_idxs.is_empty() {
        // No path: the eval harness reports shoots by hash, never by path.
        bail!("no photos found");
    }
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Shoot".into());
    let req = ImportRequest {
        events: vec![EventSpec {
            item_idxs,
            kind: EventKind::Named,
            name,
        }],
        camera_offsets: HashMap::new(),
    };
    job::validate(&scan, &req).map_err(|e| anyhow!(e))?;
    Ok((scan, req))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_disk_paths_lose_their_prefix() {
        assert_eq!(
            without_verbatim(PathBuf::from(r"\\?\E:\photos\DSLR")),
            PathBuf::from(r"E:\photos\DSLR")
        );
        assert_eq!(
            without_verbatim(PathBuf::from(r"E:\photos")),
            PathBuf::from(r"E:\photos")
        );
    }

    #[test]
    fn only_folders_strictly_inside_the_library_resolve() {
        let tmp = tempfile::tempdir().unwrap();
        let library = tmp.path().join("library");
        let shoot = library.join("2024").join("2024-05_Trip");
        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&shoot).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let resolved = resolve_in_library(&shoot, &library).unwrap();
        assert!(resolved.ends_with(r"2024\2024-05_Trip"), "{resolved:?}");
        // `..` cannot climb out once resolved.
        let sneaky = library.join("2024").join("..").join("..").join("elsewhere");
        assert!(resolve_in_library(&sneaky, &library).is_err());
        assert!(resolve_in_library(&outside, &library).is_err());
        assert!(resolve_in_library(&library, &library).is_err());
        assert!(resolve_in_library(&library.join("missing"), &library).is_err());
    }
}
