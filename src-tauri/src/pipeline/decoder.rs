use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, ImageReader};
use std::io::Cursor;
use std::path::Path;
use thiserror::Error;

use crate::pipeline::embedded;

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image decode error: {0}")]
    Image(#[from] image::ImageError),
    #[error("RAW decode error: {0}")]
    Raw(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecodeTier {
    Embedded,
    Preview,
    Full,
}

/// Decode an image file at the specified tier and return JPEG bytes.
pub fn decode_to_jpeg(path: &Path, tier: DecodeTier, quality: u8) -> Result<Vec<u8>, DecodeError> {
    if embedded::is_raw_file(path) {
        decode_raw_to_jpeg(path, tier, quality)
    } else {
        decode_standard_to_jpeg(path, tier, quality)
    }
}

fn decode_raw_to_jpeg(path: &Path, _tier: DecodeTier, _quality: u8) -> Result<Vec<u8>, DecodeError> {
    // For RAW files, all tiers return the embedded JPEG preview directly.
    // It's already a high-quality JPEG (D750: 6016x4016, ~876KB).
    // No decode/resize/re-encode needed — this is the fast path.
    embedded::extract_embedded_jpeg(path).map_err(|e| DecodeError::Raw(e.to_string()))
}

fn decode_standard_to_jpeg(path: &Path, tier: DecodeTier, quality: u8) -> Result<Vec<u8>, DecodeError> {
    // For JPEG/TIFF source files
    match tier {
        DecodeTier::Embedded => {
            // Just read and return the file as-is (it's already JPEG)
            Ok(std::fs::read(path)?)
        }
        DecodeTier::Preview => {
            let img = ImageReader::open(path)?.decode()?;
            let long_edge = img.width().max(img.height());
            if long_edge > 3000 {
                let resized = img.resize(3000, 3000, image::imageops::FilterType::Triangle);
                encode_jpeg(&resized, quality)
            } else {
                // Already small enough, return original bytes
                Ok(std::fs::read(path)?)
            }
        }
        DecodeTier::Full => {
            Ok(std::fs::read(path)?)
        }
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, DecodeError> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    img.write_with_encoder(encoder)?;
    Ok(buf.into_inner())
}

/// Generate a small thumbnail from a JPEG file (the cached preview).
pub fn generate_thumbnail_from_jpeg(jpeg_path: &Path) -> Result<Vec<u8>, DecodeError> {
    let source = ImageReader::open(jpeg_path)?.decode()?;
    let thumb = source.resize(200, 200, image::imageops::FilterType::Nearest);
    encode_jpeg(&thumb, 75)
}
