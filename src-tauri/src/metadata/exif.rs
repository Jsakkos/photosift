use exif::{Context, In, Reader, Tag, Value};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExifData {
    pub capture_time: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: Option<i32>,
    /// XMP-compatible rating (0-5), read from EXIF Rating tag (0x4746)
    /// or derived from RatingPercent (0x4749) as a fallback.
    pub rating: Option<i32>,
}

pub fn extract_exif(path: &Path) -> Result<ExifData, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let exif = Reader::new()
        .read_from_container(&mut BufReader::new(&file))
        .map_err(|e| e.to_string())?;

    let capture_time = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .map(|f| f.display_value().to_string());

    let camera_model = exif
        .get_field(Tag::Model, In::PRIMARY)
        .map(|f| f.display_value().to_string().trim_matches('"').to_string());

    let lens = exif
        .get_field(Tag::LensModel, In::PRIMARY)
        .map(|f| f.display_value().to_string().trim_matches('"').to_string());

    let focal_length = exif.get_field(Tag::FocalLength, In::PRIMARY).and_then(|f| {
        if let Value::Rational(ref v) = f.value {
            v.first().map(|r| r.to_f64())
        } else {
            None
        }
    });

    let aperture = exif.get_field(Tag::FNumber, In::PRIMARY).and_then(|f| {
        if let Value::Rational(ref v) = f.value {
            v.first().map(|r| r.to_f64())
        } else {
            None
        }
    });

    let shutter_speed = exif
        .get_field(Tag::ExposureTime, In::PRIMARY)
        .map(|f| f.display_value().to_string());

    let iso = exif
        .get_field(Tag::PhotographicSensitivity, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let width = exif
        .get_field(Tag::PixelXDimension, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::ImageWidth, In::PRIMARY))
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let height = exif
        .get_field(Tag::PixelYDimension, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::ImageLength, In::PRIMARY))
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    // Rating (0x4746) preferred; fall back to RatingPercent (0x4749) mapped per
    // Windows/Microsoft convention: 0, 25, 50, 75, 99/100 → 0..5. Both live in
    // the primary TIFF IFD.
    let rating = exif
        .get_field(Tag(Context::Tiff, 0x4746), In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32))
        .or_else(|| {
            exif.get_field(Tag(Context::Tiff, 0x4749), In::PRIMARY)
                .and_then(|f| f.value.get_uint(0).map(|v| match v {
                    0 => 0,
                    1..=24 => 1,
                    25..=49 => 2,
                    50..=74 => 3,
                    75..=98 => 4,
                    _ => 5,
                }))
        })
        .map(|r| r.clamp(0, 5));

    Ok(ExifData {
        capture_time,
        camera_model,
        lens,
        focal_length,
        aperture,
        shutter_speed,
        iso,
        width,
        height,
        orientation,
        rating,
    })
}

/// Extract the IFD1 thumbnail JPEG bytes — the small (~160×120, 10–20 KB)
/// preview every NEF/JPEG embeds at the start of the file. Used by the
/// SD-card scan path so we read 20 KB per file instead of the entire
/// 30–50 MB NEF that `preview::extract_and_decode` would pull in.
///
/// On a slow SD card this is the difference between ~1 s/file and
/// ~30 ms/file. Returns `Err` on files without an IFD1 thumbnail
/// (caller should fall back to the largest-JPEG path).
pub fn extract_ifd1_thumbnail(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file);
    let exif = Reader::new()
        .read_from_container(&mut reader)
        .map_err(|e| e.to_string())?;

    // EXIF tag 0x0201 / 0x0202 in IFD1 hold the offset/length of the
    // small thumbnail JPEG. kamadak-exif models IFD1 as `In::THUMBNAIL`.
    let offset = exif
        .get_field(Tag(Context::Tiff, 0x0201), In::THUMBNAIL)
        .and_then(|f| f.value.get_uint(0))
        .ok_or_else(|| "missing IFD1 JPEGInterchangeFormat".to_string())?;
    let length = exif
        .get_field(Tag(Context::Tiff, 0x0202), In::THUMBNAIL)
        .and_then(|f| f.value.get_uint(0))
        .ok_or_else(|| "missing IFD1 JPEGInterchangeFormatLength".to_string())?;

    if length == 0 || length > 1_000_000 {
        return Err(format!(
            "suspicious IFD1 thumbnail length: {} bytes",
            length
        ));
    }

    reader
        .seek(SeekFrom::Start(offset as u64))
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; length as usize];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;

    // Sanity check: must start with the JPEG SOI marker.
    if buf.len() < 2 || buf[0] != 0xFF || buf[1] != 0xD8 {
        return Err("IFD1 thumbnail bytes do not start with JPEG SOI".to_string());
    }

    Ok(buf)
}
