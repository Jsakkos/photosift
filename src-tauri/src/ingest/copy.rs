use sha2::{Digest, Sha256};
use std::io::{BufReader, BufWriter, Read, Write};
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

/// Stream-copy `src` to `dest` and compute its SHA-256 in a single pass,
/// so the source file (often on a slow SD card) is read exactly once.
/// Falls back to `_1`, `_2`, ... suffixed names on filename collision.
///
/// Saves a full SD-card read compared to the pre-2026-05-08 flow that
/// did `sha256_stream(src) → fs::copy(src, dest) → preview::extract(dest)`,
/// reading the source file three times.
pub fn copy_with_hash(src: &Path, dest: &Path) -> std::io::Result<(PathBuf, [u8; 32])> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let final_dest = if !dest.exists() {
        dest.to_path_buf()
    } else {
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
        let mut found: Option<PathBuf> = None;
        for i in 1..1000 {
            let candidate = parent.join(format!("{}_{}{}", stem, i, ext));
            if !candidate.exists() {
                found = Some(candidate);
                break;
            }
        }
        match found {
            Some(p) => p,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "Too many filename collisions",
                ));
            }
        }
    };

    // 64 KB matches typical SD-card sequential-read sweet-spot. Smaller
    // buffers (8 KB, the old SHA streamer's choice) cap throughput on
    // class-10/UHS-I cards at well below their rated speed.
    const BUF: usize = 64 * 1024;
    let src_file = std::fs::File::open(src)?;
    let mut reader = BufReader::with_capacity(BUF, src_file);
    let dest_file = std::fs::File::create(&final_dest)?;
    let mut writer = BufWriter::with_capacity(BUF, dest_file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUF];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
    }

    writer.flush()?;
    drop(writer);

    Ok((final_dest, hasher.finalize().into()))
}

/// Get the pictures library root. Falls back to ~/Pictures.
pub fn library_root() -> PathBuf {
    dirs::picture_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Pictures")
    })
}
