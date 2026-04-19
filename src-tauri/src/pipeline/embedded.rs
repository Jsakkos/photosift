use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbeddedError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not a valid TIFF/NEF file")]
    InvalidTiff,
    #[error("No embedded JPEG found")]
    NoJpeg,
}

/// Extract the largest embedded JPEG preview from a NEF (TIFF-based) file.
/// Retained for callers that only need the primary preview bytes. For
/// decode-capable callers, use `extract_all_jpegs` and try each in order.
pub fn extract_embedded_jpeg(path: &Path) -> Result<Vec<u8>, EmbeddedError> {
    let mut all = extract_all_jpegs(path)?;
    if all.is_empty() {
        return Err(EmbeddedError::NoJpeg);
    }
    Ok(all.remove(0).bytes)
}

pub struct EmbeddedJpeg {
    pub bytes: Vec<u8>,
}

/// Extract every JPEG blob in the file, sorted largest-first. NEFs
/// typically contain both a full-resolution preview (often in an
/// exotic format Nikon invented — arithmetic coding, DNL markers,
/// 12-bit precision) and a smaller standard-baseline thumbnail. When
/// the primary won't decode, the smaller one usually will.
pub fn extract_all_jpegs(path: &Path) -> Result<Vec<EmbeddedJpeg>, EmbeddedError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut header = [0u8; 4];
    file.read_exact(&mut header)?;

    let magic = match &header[0..2] {
        b"II" => u16::from_le_bytes([header[2], header[3]]),
        b"MM" => u16::from_be_bytes([header[2], header[3]]),
        _ => return Err(EmbeddedError::InvalidTiff),
    };
    if magic != 42 {
        return Err(EmbeddedError::InvalidTiff);
    }

    file.seek(SeekFrom::Start(0))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    let mut jpegs: Vec<EmbeddedJpeg> = Vec::new();
    let mut i = 0;

    while i < data.len().saturating_sub(4) {
        if data[i] == 0xFF
            && data[i + 1] == 0xD8
            && data[i + 2] == 0xFF
            && is_valid_jpeg_first_marker(data[i + 3])
        {
            if let Some(end) = find_jpeg_end(&data, i) {
                let jpeg_data = &data[i..=end];
                // Only consider blobs > 10KB (skip tiny EXIF thumbnails).
                if jpeg_data.len() > 10_000 {
                    jpegs.push(EmbeddedJpeg {
                        bytes: jpeg_data.to_vec(),
                    });
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    if jpegs.is_empty() {
        return Err(EmbeddedError::NoJpeg);
    }
    jpegs.sort_by(|a, b| b.bytes.len().cmp(&a.bytes.len()));
    Ok(jpegs)
}

fn find_jpeg_end(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 2;
    while i < data.len().saturating_sub(1) {
        if data[i] == 0xFF {
            match data[i + 1] {
                0xD9 => return Some(i + 1),
                0x00 | 0xD0..=0xD8 => { i += 2; }
                _ => {
                    if i + 3 < data.len() {
                        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                        i += 2 + len;
                    } else {
                        return None;
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

/// After FF D8 FF, the fourth byte must be a standard JPEG marker.
/// Rejects false positives from raw sensor data that coincidentally contain FF D8 FF.
pub fn is_valid_jpeg_first_marker(byte: u8) -> bool {
    matches!(byte,
        0xC0..=0xCF | 0xDB | 0xDD | 0xE0..=0xEF | 0xFE
    )
}

pub fn is_raw_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("nef"))
        .unwrap_or(false)
}

pub fn is_supported_image(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "nef" | "jpg" | "jpeg" | "tif" | "tiff"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_raw_file() {
        assert!(is_raw_file(Path::new("photo.nef")));
        assert!(is_raw_file(Path::new("photo.NEF")));
        assert!(!is_raw_file(Path::new("photo.jpg")));
    }

    #[test]
    fn test_is_supported_image() {
        assert!(is_supported_image(Path::new("a.nef")));
        assert!(is_supported_image(Path::new("a.jpg")));
        assert!(is_supported_image(Path::new("a.JPEG")));
        assert!(is_supported_image(Path::new("a.tif")));
        assert!(!is_supported_image(Path::new("a.png")));
        assert!(!is_supported_image(Path::new("a.txt")));
    }
}
