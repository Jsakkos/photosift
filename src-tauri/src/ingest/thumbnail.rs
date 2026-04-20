use image::{DynamicImage, GenericImageView};
use jpeg_encoder::{ColorType, Encoder as JpegEncoder};

/// Generate a 512px (longest edge) JPEG thumbnail from a decoded image.
pub fn make_thumb(img: &DynamicImage) -> Result<Vec<u8>, String> {
    let thumb = img.thumbnail(512, 512);
    let (w, h) = thumb.dimensions();
    let rgb = thumb.to_rgb8();
    let mut out = Vec::new();
    let encoder = JpegEncoder::new(&mut out, 82);
    encoder
        .encode(rgb.as_raw(), w as u16, h as u16, ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}
