use crate::pipeline::embedded;
use image::{DynamicImage, ImageBuffer, Rgb};
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
///
/// Nikon NEFs frequently embed JPEGs using arithmetic coding (DAC
/// segment), DNL markers, or 12+-bit precision — none of which the
/// default `image` crate decoder supports. Fall back to `zune-jpeg`,
/// which handles those variants. If both decoders fail, return an
/// error; callers already tolerate this (the raw preview still gets
/// saved, thumb/phash just get skipped).
pub fn decode_jpeg_to_image(jpeg_bytes: &[u8]) -> Result<DynamicImage, String> {
    match image::load_from_memory(jpeg_bytes) {
        Ok(img) => Ok(img),
        Err(std_err) => decode_with_zune(jpeg_bytes).map_err(|zune_err| {
            format!(
                "image crate: {}; zune-jpeg: {}",
                std_err, zune_err
            )
        }),
    }
}

fn decode_with_zune(jpeg_bytes: &[u8]) -> Result<DynamicImage, String> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    use zune_jpeg::JpegDecoder;

    let mut decoder = JpegDecoder::new_with_options(
        jpeg_bytes,
        DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB),
    );
    let pixels = decoder.decode().map_err(|e| e.to_string())?;
    let info = decoder
        .info()
        .ok_or_else(|| "zune-jpeg returned no dimensions".to_string())?;
    let w = info.width as u32;
    let h = info.height as u32;
    let expected = (w as usize) * (h as usize) * 3;
    if pixels.len() != expected {
        return Err(format!(
            "zune-jpeg pixel count mismatch: got {}, expected {}",
            pixels.len(),
            expected
        ));
    }
    let buf: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(w, h, pixels).ok_or_else(|| "ImageBuffer::from_raw failed".to_string())?;
    Ok(DynamicImage::ImageRgb8(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{codecs::jpeg::JpegEncoder, ImageBuffer, Luma};
    use std::io::Cursor;

    fn tiny_standard_jpeg() -> Vec<u8> {
        let img: ImageBuffer<Luma<u8>, Vec<u8>> =
            ImageBuffer::from_fn(16, 16, |x, y| Luma([((x + y) * 8) as u8]));
        let mut buf = Cursor::new(Vec::new());
        let encoder = JpegEncoder::new_with_quality(&mut buf, 80);
        img.write_with_encoder(encoder).unwrap();
        buf.into_inner()
    }

    #[test]
    fn test_decode_standard_jpeg_succeeds_via_primary() {
        let bytes = tiny_standard_jpeg();
        let img = decode_jpeg_to_image(&bytes).unwrap();
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_decode_garbage_fails_cleanly() {
        let err = decode_jpeg_to_image(b"this is not a jpeg at all").unwrap_err();
        // Error message should mention both fallback paths were tried.
        assert!(err.contains("image crate"));
        assert!(err.contains("zune-jpeg"));
    }
}
