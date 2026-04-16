use crate::pipeline::embedded;
use image::DynamicImage;
use std::path::Path;

/// Extract the raw JPEG bytes from a file without decoding.
/// For RAW files: extracts the embedded JPEG preview.
/// For JPEG/TIFF: reads the file as-is.
pub fn extract_jpeg_bytes(path: &Path) -> Result<Vec<u8>, String> {
    if embedded::is_raw_file(path) {
        embedded::extract_embedded_jpeg(path).map_err(|e| e.to_string())
    } else {
        std::fs::read(path).map_err(|e| e.to_string())
    }
}

/// Decode JPEG bytes to a DynamicImage for thumbnail generation and pHash.
/// Files with non-standard JPEG markers (arithmetic coding, DNL, etc.)
/// will fail — their previews are still served via the raw bytes.
pub fn decode_jpeg_to_image(jpeg_bytes: &[u8]) -> Result<DynamicImage, String> {
    image::load_from_memory(jpeg_bytes).map_err(|e| e.to_string())
}
