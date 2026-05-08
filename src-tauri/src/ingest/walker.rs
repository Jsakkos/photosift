use crate::pipeline::embedded;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Walk a source directory and collect all supported image files, sorted by
/// filename DESCENDING. Newest-named files (e.g. DSC_4079.NEF) come first so
/// the rayon scan pool processes the user's most recent shots first — on a
/// 1700-file SD card with years of history, this means today's day appears
/// in the import dialog within seconds instead of after a full scan.
pub fn walk_source(source: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(source)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && embedded::is_supported_image(p))
        .collect();
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    files
}
