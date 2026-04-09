use image::codecs::jpeg::JpegEncoder;
use image::io::Reader as ImageReader2;
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
    #[error("Unsupported format: {0}")]
    Unsupported(String),
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

fn decode_raw_to_jpeg(path: &Path, tier: DecodeTier, quality: u8) -> Result<Vec<u8>, DecodeError> {
    match tier {
        DecodeTier::Embedded => {
            // Extract embedded JPEG from RAW — no further processing
            embedded::extract_embedded_jpeg(path).map_err(|e| DecodeError::Raw(e.to_string()))
        }
        DecodeTier::Preview | DecodeTier::Full => {
            // For Phase 1, use the embedded JPEG as the source and resize.
            // Full rawler pixel-level decode will be added when wgpu compute is integrated.
            let jpeg_bytes = embedded::extract_embedded_jpeg(path)
                .map_err(|e| DecodeError::Raw(e.to_string()))?;
            let img = load_jpeg_from_memory(&jpeg_bytes)?;

            let img = if tier == DecodeTier::Preview {
                let long_edge = img.width().max(img.height());
                if long_edge > 3000 {
                    img.resize(3000, 3000, image::imageops::FilterType::Lanczos3)
                } else {
                    img
                }
            } else {
                img
            };

            encode_jpeg(&img, quality)
        }
    }
}

fn decode_standard_to_jpeg(path: &Path, tier: DecodeTier, quality: u8) -> Result<Vec<u8>, DecodeError> {
    let img = ImageReader::open(path)?.decode()?;

    let img = match tier {
        DecodeTier::Embedded | DecodeTier::Preview => {
            let long_edge = img.width().max(img.height());
            let target = if tier == DecodeTier::Embedded { 1600 } else { 3000 };
            if long_edge > target {
                img.resize(target, target, image::imageops::FilterType::Lanczos3)
            } else {
                img
            }
        }
        DecodeTier::Full => img,
    };

    encode_jpeg(&img, quality)
}

/// Load JPEG bytes with explicit format hint (avoids format detection failures
/// on embedded JPEG blobs extracted from NEF files).
fn load_jpeg_from_memory(bytes: &[u8]) -> Result<DynamicImage, DecodeError> {
    // Try with explicit JPEG hint first
    let reader = ImageReader2::with_format(Cursor::new(bytes), ImageFormat::Jpeg);
    match reader.decode() {
        Ok(img) => Ok(img),
        Err(_) => {
            // Fallback: try with format guessing (handles TIFF thumbnails etc.)
            let reader = ImageReader2::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| DecodeError::Io(e))?;
            Ok(reader.decode()?)
        }
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, DecodeError> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    img.write_with_encoder(encoder)?;
    Ok(buf.into_inner())
}

/// Generate a small thumbnail (200px long edge) from any supported image.
pub fn generate_thumbnail(path: &Path) -> Result<Vec<u8>, DecodeError> {
    let source = if embedded::is_raw_file(path) {
        let jpeg_bytes = embedded::extract_embedded_jpeg(path)
            .map_err(|e| DecodeError::Raw(e.to_string()))?;
        load_jpeg_from_memory(&jpeg_bytes)?
    } else {
        ImageReader::open(path)?.decode()?
    };

    let thumb = source.resize(200, 200, image::imageops::FilterType::Triangle);
    encode_jpeg(&thumb, 80)
}
