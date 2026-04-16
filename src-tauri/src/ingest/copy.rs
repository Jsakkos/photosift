use std::path::{Path, PathBuf};

/// Build the destination path for a file within a shoot folder: {shoot_folder}/RAW/{filename}
pub fn plan_dest(shoot_folder: &Path, filename: &str) -> PathBuf {
    shoot_folder.join("RAW").join(filename)
}

/// Derive the shoot folder path, appending _2, _3 etc. if the folder already exists.
pub fn shoot_folder(library_root: &Path, yyyy_mm: &str, slug: &str) -> PathBuf {
    let yyyy = &yyyy_mm[..4];
    let base = format!("{}_{}", yyyy_mm, slug);
    let parent = library_root.join("DSLR").join(yyyy);
    let candidate = parent.join(&base);
    if !candidate.exists() {
        return candidate;
    }
    for i in 2..100 {
        let suffixed = parent.join(format!("{}_{}", base, i));
        if !suffixed.exists() {
            return suffixed;
        }
    }
    candidate
}

/// Copy src to dest, creating parent directories. Returns the final dest path.
/// If dest already exists, appends _1, _2, etc. to the stem to avoid collision.
pub fn copy_file(src: &Path, dest: &Path) -> std::io::Result<PathBuf> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !dest.exists() {
        std::fs::copy(src, dest)?;
        return Ok(dest.to_path_buf());
    }
    let stem = dest
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = dest
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = dest.parent().unwrap_or(Path::new("."));
    for i in 1..1000 {
        let candidate = parent.join(format!("{}_{}{}", stem, i, ext));
        if !candidate.exists() {
            std::fs::copy(src, &candidate)?;
            return Ok(candidate);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "Too many filename collisions",
    ))
}

/// Get the pictures library root. Falls back to ~/Pictures.
pub fn library_root() -> PathBuf {
    dirs::picture_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Pictures")
    })
}
