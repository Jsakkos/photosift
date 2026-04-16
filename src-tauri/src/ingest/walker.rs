use crate::pipeline::embedded;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Walk a source directory and collect all supported image files, sorted by name.
pub fn walk_source(source: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(source)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && embedded::is_supported_image(p))
        .collect();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    files
}
