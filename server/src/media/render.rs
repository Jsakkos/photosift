//! Decode an embedded/plain JPEG once, then write the oriented 2560 px preview
//! and 400 px thumbnail the UI serves. Previews are never upscaled.

use std::path::Path;

use fast_image_resize::{
    images::{Image, ImageRef},
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
};
use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};

use super::MediaError;

const PREVIEW_EDGE: u32 = 2560;
const THUMB_EDGE: u32 = 400;
const PREVIEW_QUALITY: u8 = 85;
const THUMB_QUALITY: u8 = 80;
/// Cameras top out far below this; the cap bounds memory on a malformed header.
const MAX_DIMENSION: usize = 16384;

#[derive(Debug)]
pub struct Rendered {
    /// Source dimensions after orientation: what the photo looks like upright.
    pub display_w: u32,
    pub display_h: u32,
    /// Oriented thumbnail pixels (RGB8) so the caller can hash without re-decoding.
    pub thumb_rgb: Vec<u8>,
    pub thumb_w: u32,
    pub thumb_h: u32,
}

/// Packed RGB8 pixels, row-major.
pub struct Rgb {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

/// The name analysis code (faces, identity) uses for decoded pixels.
pub type RgbImage = Rgb;

pub fn make_previews(
    jpeg: &[u8],
    orientation: u16,
    preview_out: &Path,
    thumb_out: &Path,
) -> Result<Rendered, MediaError> {
    let full = decode(jpeg)?;
    let (src_w, src_h) = (full.w, full.h);

    // Orienting after the shrink touches ~4M pixels instead of ~24M.
    let preview = orient(fit(full, PREVIEW_EDGE)?, orientation);
    let thumb = fit_copy(&preview, THUMB_EDGE)?;

    write_jpeg(&preview, PREVIEW_QUALITY, preview_out)?;
    write_jpeg(&thumb, THUMB_QUALITY, thumb_out)?;

    let (display_w, display_h) = if swaps_axes(orientation) {
        (src_h, src_w)
    } else {
        (src_w, src_h)
    };
    Ok(Rendered {
        display_w,
        display_h,
        thumb_w: thumb.w,
        thumb_h: thumb.h,
        thumb_rgb: thumb.px,
    })
}

/// An oriented thumbnail as JPEG bytes, for pictures that have no row yet
/// (the import screen shows card contents before anything is copied).
pub fn thumbnail_jpeg(jpeg: &[u8], orientation: u16) -> Result<Vec<u8>, MediaError> {
    let thumb = orient(fit(decode(jpeg)?, THUMB_EDGE)?, orientation);
    encode_jpeg(&thumb, THUMB_QUALITY)
}

pub fn decode_jpeg(bytes: &[u8]) -> Result<Rgb, MediaError> {
    decode(bytes)
}

fn decode(bytes: &[u8]) -> Result<Rgb, MediaError> {
    // Non-strict: Nikon NEF previews carry trailing/odd data that strict mode rejects.
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_strict_mode(false)
        .set_max_width(MAX_DIMENSION)
        .set_max_height(MAX_DIMENSION);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(bytes, options);
    let px = decoder
        .decode()
        .map_err(|e| MediaError::Decode(format!("{e:?}")))?;
    let info = decoder
        .info()
        .ok_or_else(|| MediaError::Decode("no image info".into()))?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    if px.len() != w as usize * h as usize * 3 {
        return Err(MediaError::Decode(format!(
            "unexpected pixel buffer for {w}x{h}"
        )));
    }
    Ok(Rgb { w, h, px })
}

fn resize(src: &Rgb, w: u32, h: u32) -> Result<Rgb, MediaError> {
    resize_with(src, w, h, FilterType::CatmullRom)
}

/// Resizes to exactly `w`x`h`. Box averages fixed rectangles (cheap, alias-free
/// for big shrinks: used for analysis); CatmullRom keeps display detail.
pub fn resize_with(src: &Rgb, w: u32, h: u32, filter: FilterType) -> Result<Rgb, MediaError> {
    let err = |e: &dyn std::fmt::Display| MediaError::Resize(e.to_string());
    let from = ImageRef::new(src.w, src.h, &src.px, PixelType::U8x3).map_err(|e| err(&e))?;
    let mut to = Image::new(w, h, PixelType::U8x3);
    Resizer::new()
        .resize(
            &from,
            &mut to,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter)),
        )
        .map_err(|e| err(&e))?;
    Ok(Rgb {
        w,
        h,
        px: to.into_vec(),
    })
}

pub(crate) fn target_size(src: &Rgb, edge: u32) -> Option<(u32, u32)> {
    let long = src.w.max(src.h);
    if long <= edge {
        return None;
    }
    let scale = f64::from(edge) / f64::from(long);
    let scaled = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    Some((scaled(src.w), scaled(src.h)))
}

/// Shrinks so the long edge is `edge`; returns `src` untouched when already small enough.
fn fit(src: Rgb, edge: u32) -> Result<Rgb, MediaError> {
    match target_size(&src, edge) {
        Some((w, h)) => resize(&src, w, h),
        None => Ok(src),
    }
}

fn fit_copy(src: &Rgb, edge: u32) -> Result<Rgb, MediaError> {
    match target_size(src, edge) {
        Some((w, h)) => resize(src, w, h),
        None => Ok(Rgb {
            w: src.w,
            h: src.h,
            px: src.px.clone(),
        }),
    }
}

/// EXIF orientations 5-8 transpose the image, so width and height trade places.
fn swaps_axes(orientation: u16) -> bool {
    (5..=8).contains(&orientation)
}

/// Applies EXIF orientation 1-8; any other value leaves the pixels as they are.
fn orient(src: Rgb, orientation: u16) -> Rgb {
    if !(2..=8).contains(&orientation) {
        return src;
    }
    let (w, h) = (src.w as usize, src.h as usize);
    let (ow, oh) = if swaps_axes(orientation) {
        (h, w)
    } else {
        (w, h)
    };
    let mut out = vec![0u8; src.px.len()];
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = match orientation {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (y, h - 1 - x),
                7 => (w - 1 - y, h - 1 - x),
                _ => (w - 1 - y, x),
            };
            let from = (sy * w + sx) * 3;
            let to = (y * ow + x) * 3;
            out[to..to + 3].copy_from_slice(&src.px[from..from + 3]);
        }
    }
    Rgb {
        w: ow as u32,
        h: oh as u32,
        px: out,
    }
}

pub(crate) fn encode_jpeg(img: &Rgb, quality: u8) -> Result<Vec<u8>, MediaError> {
    let too_big = || MediaError::Encode(format!("{}x{} exceeds JPEG limits", img.w, img.h));
    let (w, h) = (
        u16::try_from(img.w).map_err(|_| too_big())?,
        u16::try_from(img.h).map_err(|_| too_big())?,
    );
    let mut buf = Vec::new();
    jpeg_encoder::Encoder::new(&mut buf, quality)
        .encode(&img.px, w, h, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| MediaError::Encode(e.to_string()))?;
    Ok(buf)
}

/// Encodes then renames into place so a crash never leaves a half-written
/// file that a later "already rendered?" check would trust.
fn write_jpeg(img: &Rgb, quality: u8, path: &Path) -> Result<(), MediaError> {
    let buf = encode_jpeg(img, quality)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("jpg.tmp");
    std::fs::write(&tmp, &buf)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3x2 image whose pixel value encodes its position: [x + 10*y] in the red channel.
    fn sample() -> Rgb {
        let mut px = Vec::new();
        for y in 0..2u8 {
            for x in 0..3u8 {
                px.extend_from_slice(&[x + 10 * y, 0, 0]);
            }
        }
        Rgb { w: 3, h: 2, px }
    }

    fn reds(img: &Rgb) -> Vec<u8> {
        img.px.chunks(3).map(|p| p[0]).collect()
    }

    #[test]
    fn orient_matches_exif_semantics() {
        // Source rows: [0 1 2] / [10 11 12]
        let cases: [(u16, (u32, u32), Vec<u8>); 7] = [
            (2, (3, 2), vec![2, 1, 0, 12, 11, 10]),
            (3, (3, 2), vec![12, 11, 10, 2, 1, 0]),
            (4, (3, 2), vec![10, 11, 12, 0, 1, 2]),
            (5, (2, 3), vec![0, 10, 1, 11, 2, 12]),
            (6, (2, 3), vec![10, 0, 11, 1, 12, 2]),
            (7, (2, 3), vec![12, 2, 11, 1, 10, 0]),
            (8, (2, 3), vec![2, 12, 1, 11, 0, 10]),
        ];
        for (o, dims, expected) in cases {
            let out = orient(sample(), o);
            assert_eq!((out.w, out.h), dims, "orientation {o} dims");
            assert_eq!(reds(&out), expected, "orientation {o} pixels");
        }
        assert_eq!(reds(&orient(sample(), 1)), vec![0, 1, 2, 10, 11, 12]);
        assert_eq!(reds(&orient(sample(), 0)), vec![0, 1, 2, 10, 11, 12]);
    }
}
