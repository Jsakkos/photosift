//! Derived pictures for the Claude helper: downsized previews and face crops.

use std::path::Path;

use fast_image_resize::FilterType;

use super::{
    render::{decode_jpeg, encode_jpeg, resize_with, target_size, Rgb},
    MediaError,
};
use crate::db::photos::FaceRow;

const DOWNSIZED_QUALITY: u8 = 80;
const CROP_QUALITY: u8 = 85;
/// Context kept around the face box, as a fraction of the box size per side.
const FACE_PAD: f32 = 0.4;

/// The preview shrunk so its long edge is at most `edge` (never upscaled).
pub fn downsized(preview_path: &Path, edge: u32) -> Result<Vec<u8>, MediaError> {
    let img = decode_jpeg(&std::fs::read(preview_path)?)?;
    let img = match target_size(&img, edge) {
        Some((w, h)) => resize_with(&img, w, h, FilterType::CatmullRom)?,
        None => img,
    };
    encode_jpeg(&img, DOWNSIZED_QUALITY)
}

/// A square `edge`x`edge` crop around the face: the box padded by 40% per
/// side, kept inside the frame (shifted, or shrunk when the frame is too
/// small), so a face at a corner never panics or leaves the picture.
pub fn face_crop(preview: &Rgb, face: &FaceRow, edge: u32) -> Result<Vec<u8>, MediaError> {
    let (fw, fh) = (preview.w as f32, preview.h as f32);
    let (bx, by, bw, bh) = face.bbox;
    let (cx, cy) = ((bx + bw / 2.0) * fw, (by + bh / 2.0) * fh);
    let side = (bw * fw).max(bh * fh) * (1.0 + 2.0 * FACE_PAD);
    let side = side.clamp(1.0, fw.min(fh));
    let half = side / 2.0;
    let x0 = (cx - half).clamp(0.0, fw - side);
    let y0 = (cy - half).clamp(0.0, fh - side);
    let (x0, y0) = (x0.round() as u32, y0.round() as u32);
    let side = (side.round() as u32)
        .max(1)
        .min(preview.w - x0)
        .min(preview.h - y0);

    let mut px = Vec::with_capacity(side as usize * side as usize * 3);
    for row in y0..y0 + side {
        let start = (row as usize * preview.w as usize + x0 as usize) * 3;
        px.extend_from_slice(&preview.px[start..start + side as usize * 3]);
    }
    let cropped = Rgb {
        w: side,
        h: side,
        px,
    };
    let out = resize_with(&cropped, edge, edge, FilterType::CatmullRom)?;
    encode_jpeg(&out, CROP_QUALITY)
}
