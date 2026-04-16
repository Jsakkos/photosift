use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;
use std::io::Cursor;

/// Generate a 512px (longest edge) JPEG thumbnail from a decoded image.
pub fn make_thumb(img: &DynamicImage) -> Result<Vec<u8>, String> {
    let thumb = img.thumbnail(512, 512);
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, 82);
    thumb
        .write_with_encoder(encoder)
        .map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}
