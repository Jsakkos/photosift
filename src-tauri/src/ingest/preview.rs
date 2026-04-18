use crate::pipeline::embedded;
use image::{DynamicImage, ImageBuffer, Rgb};
use std::path::Path;

/// Extract JPEG bytes suitable for saving as the preview file + decoding.
/// Returns (preview_bytes_for_disk, decodable_image).
///
/// For RAW files the strategy is:
/// 1. Walk every embedded JPEG candidate largest-first.
/// 2. The first candidate that BOTH decodes AND is reasonably sized
///    becomes the disk preview (so downstream readers — protocol
///    handler, AI worker, loupe — can always decode it).
/// 3. If no candidate decodes, keep the largest raw bytes anyway so
///    SOMETHING lands on disk, even if tools can't read it.
///
/// For JPEG/TIFF source files, there's one JPEG and both values
/// come from it.
pub fn extract_and_decode(path: &Path) -> Result<(Vec<u8>, Option<DynamicImage>), String> {
    if embedded::is_raw_file(path) {
        let candidates = embedded::extract_all_jpegs(path).map_err(|e| e.to_string())?;
        if candidates.is_empty() {
            return Err("no embedded JPEG".to_string());
        }

        // Try to find a candidate that decodes. If found, save ITS bytes
        // as the preview — not the largest raw bytes — so the preview
        // file on disk is always decodable by downstream readers (AI
        // worker calls image::open, loupe served by the webview).
        let mut last_err = String::new();
        for cand in &candidates {
            match decode_jpeg_to_image(&cand.bytes) {
                Ok(img) => return Ok((cand.bytes.clone(), Some(img))),
                Err(e) => last_err = e,
            }
        }

        // Nothing decoded. Fall back to the primary (full-res) bytes
        // just so the preview file isn't missing; downstream will skip
        // thumb/phash/AI for these photos.
        log::warn!(
            "All {} embedded JPEG candidates for {:?} failed to decode. Last error: {}",
            candidates.len(),
            path,
            last_err
        );
        Ok((candidates[0].bytes.clone(), None))
    } else {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let img = decode_jpeg_to_image(&bytes).ok();
        Ok((bytes, img))
    }
}

/// Extract the raw JPEG bytes from a file without decoding.
/// Kept for callers (including the protocol handler's re-extract
/// path) that don't need the decoded image.
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

    // Strict mode rejects the DNL marker + "extra bytes between headers"
    // that Nikon NEFs routinely emit. Turning it off lets zune tolerate
    // those variants while still catching genuine corruption.
    let mut decoder = JpegDecoder::new_with_options(
        jpeg_bytes,
        DecoderOptions::default()
            .jpeg_set_out_colorspace(ColorSpace::RGB)
            .set_strict_mode(false),
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
