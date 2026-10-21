//! EXIF fields the importer needs, read from in-memory bytes. Works on a file
//! prefix, because scanning a card only reads the first few hundred KiB.

use std::io::Cursor;
use std::path::Path;

use chrono::NaiveDateTime;
use exif::{Exif, In, Reader, Tag, Value};

use super::embedded;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExifInfo {
    pub capture_time: Option<NaiveDateTime>,
    /// `"{Make} {Model} #{BodySerialNumber}"`, without the serial when the body reports none.
    pub camera: Option<String>,
    /// EXIF orientation 1-8; 1 when absent or out of range.
    pub orientation: u16,
    /// Stored pixel dimensions before orientation; 0 when the file does not say.
    pub width: u32,
    pub height: u32,
}

impl Default for ExifInfo {
    fn default() -> Self {
        Self {
            capture_time: None,
            camera: None,
            orientation: 1,
            width: 0,
            height: 0,
        }
    }
}

/// Never fails: a file with no readable EXIF yields defaults so the caller can
/// fall back to mtime rather than dropping the photo.
pub fn read(bytes: &[u8], path: &Path) -> ExifInfo {
    let is_raf = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("raf"));

    // RAF is not a TIFF/JPEG container, but the camera JPEG inside it carries
    // the standard EXIF block at its very start.
    let container = if is_raf { raf_jpeg_bytes(bytes) } else { bytes };

    match Reader::new().read_from_container(&mut Cursor::new(container)) {
        Ok(exif) => from_exif(&exif),
        Err(_) => ExifInfo::default(),
    }
}

/// The embedded JPEG, clipped to what is present so a truncated prefix still works.
fn raf_jpeg_bytes(bytes: &[u8]) -> &[u8] {
    if let Some((off, len)) = embedded::raf_header_range(bytes) {
        return &bytes[off..off.saturating_add(len).min(bytes.len())];
    }
    embedded::largest_jpeg(bytes, "raf").unwrap_or(bytes)
}

fn from_exif(exif: &Exif) -> ExifInfo {
    let ascii = |tag: Tag| -> Option<String> {
        let field = exif.get_field(tag, In::PRIMARY)?;
        let Value::Ascii(parts) = &field.value else {
            return None;
        };
        let s = String::from_utf8_lossy(parts.first()?);
        let s = s.trim_matches(|c: char| c == '\0' || c.is_whitespace());
        (!s.is_empty()).then(|| s.to_string())
    };
    let uint = |tag: Tag| {
        exif.get_field(tag, In::PRIMARY)
            .and_then(|f| f.value.get_uint(0))
    };

    let capture_time = ascii(Tag::DateTimeOriginal)
        .and_then(|s| NaiveDateTime::parse_from_str(&s, "%Y:%m:%d %H:%M:%S").ok());

    let mut name: Vec<String> = [Tag::Make, Tag::Model]
        .into_iter()
        .filter_map(ascii)
        .collect();
    if let Some(serial) = ascii(Tag::BodySerialNumber) {
        name.push(format!("#{serial}"));
    }
    let camera = (!name.is_empty()).then(|| name.join(" "));

    let orientation = uint(Tag::Orientation)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);

    ExifInfo {
        capture_time,
        camera,
        orientation,
        width: uint(Tag::PixelXDimension)
            .or_else(|| uint(Tag::ImageWidth))
            .unwrap_or(0),
        height: uint(Tag::PixelYDimension)
            .or_else(|| uint(Tag::ImageLength))
            .unwrap_or(0),
    }
}
