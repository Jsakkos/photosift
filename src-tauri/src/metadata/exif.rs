use exif::{Context, In, Reader, Tag, Value};
use std::fs::File;
use std::io::BufReader;
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
