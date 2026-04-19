use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;
use std::io::Cursor;

/// Apply EXIF orientation (1-8) to produce an upright `DynamicImage`.
///
/// Returns the input unchanged for orientation 1, `None`, or any value
/// outside the 1–8 range. Rotation uses the `image` crate's lossless
/// transforms so colour data is preserved; this is always cheap
/// relative to decode/encode.
///
/// EXIF orientation semantics (from the TIFF/EXIF spec):
/// - 1 = normal
/// - 2 = mirrored horizontally
/// - 3 = rotated 180°
/// - 4 = mirrored vertically
/// - 5 = mirrored horizontally, then rotated 90° CW ("transpose")
/// - 6 = rotated 90° CW
/// - 7 = mirrored horizontally, then rotated 90° CCW ("transverse")
/// - 8 = rotated 90° CCW
pub fn apply(img: DynamicImage, orientation: Option<i32>) -> DynamicImage {
    match orientation {
        Some(2) => img.fliph(),
        Some(3) => img.rotate180(),
        Some(4) => img.flipv(),
        Some(5) => img.rotate90().fliph(),
        Some(6) => img.rotate90(),
        Some(7) => img.rotate270().fliph(),
        Some(8) => img.rotate270(),
        _ => img,
    }
}

/// Apply orientation to the decoded image and, when rotation was non-trivial,
/// re-encode JPEG bytes so the on-disk preview matches.
///
/// For orientation 1 / `None` / unknown values the original bytes are returned
/// unchanged (zero re-encode cost). For 2–8 the rotated image is encoded at
/// quality 92 so the loupe view sees a near-lossless upright preview.
pub fn apply_and_reencode(
    img: DynamicImage,
    orientation: Option<i32>,
    original_bytes: Vec<u8>,
) -> Result<(Vec<u8>, DynamicImage), String> {
    match orientation {
        Some(n) if (2..=8).contains(&n) => {
            let rotated = apply(img, orientation);
            let bytes = encode_jpeg(&rotated, 92)?;
            Ok((bytes, rotated))
        }
        _ => Ok((original_bytes, img)),
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    img.write_with_encoder(encoder).map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// Human-readable short label for an EXIF orientation value, for UI display.
/// Returns `None` when there's nothing to surface (orientation 1, missing, or unknown).
pub fn label(orientation: Option<i32>) -> Option<&'static str> {
    match orientation {
        Some(2) => Some("flipped horizontal"),
        Some(3) => Some("rotated 180°"),
        Some(4) => Some("flipped vertical"),
        Some(5) => Some("transposed"),
        Some(6) => Some("rotated 90° CW"),
        Some(7) => Some("transversed"),
        Some(8) => Some("rotated 90° CCW"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageBuffer, Rgb};

    /// Build a 3×2 image with distinct per-pixel colours so we can
    /// track where each pixel lands after a rotation or flip.
    ///
    /// Layout (x, y):
    ///   (0,0)=R  (1,0)=G  (2,0)=B
    ///   (0,1)=C  (1,1)=M  (2,1)=Y
    fn fixture() -> DynamicImage {
        let mut buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(3, 2);
        buf.put_pixel(0, 0, Rgb([255, 0, 0])); // R
        buf.put_pixel(1, 0, Rgb([0, 255, 0])); // G
        buf.put_pixel(2, 0, Rgb([0, 0, 255])); // B
        buf.put_pixel(0, 1, Rgb([0, 255, 255])); // C
        buf.put_pixel(1, 1, Rgb([255, 0, 255])); // M
        buf.put_pixel(2, 1, Rgb([255, 255, 0])); // Y
        DynamicImage::ImageRgb8(buf)
    }

    fn px(img: &DynamicImage, x: u32, y: u32) -> [u8; 3] {
        let p = img.to_rgb8().get_pixel(x, y).0;
        [p[0], p[1], p[2]]
    }

    const R: [u8; 3] = [255, 0, 0];
    const G: [u8; 3] = [0, 255, 0];
    const B: [u8; 3] = [0, 0, 255];
    const C: [u8; 3] = [0, 255, 255];
    const M: [u8; 3] = [255, 0, 255];
    const Y: [u8; 3] = [255, 255, 0];

    #[test]
    fn orientation_1_is_identity() {
        let out = apply(fixture(), Some(1));
        assert_eq!(out.dimensions(), (3, 2));
        assert_eq!(px(&out, 0, 0), R);
        assert_eq!(px(&out, 2, 1), Y);
    }

    #[test]
    fn orientation_none_is_identity() {
        let out = apply(fixture(), None);
        assert_eq!(out.dimensions(), (3, 2));
        assert_eq!(px(&out, 0, 0), R);
    }

    #[test]
    fn orientation_unknown_is_identity() {
        let out = apply(fixture(), Some(99));
        assert_eq!(out.dimensions(), (3, 2));
        assert_eq!(px(&out, 0, 0), R);
    }

    #[test]
    fn orientation_2_flips_horizontal() {
        // Mirror across vertical axis: column 0 swaps with column 2.
        let out = apply(fixture(), Some(2));
        assert_eq!(out.dimensions(), (3, 2));
        assert_eq!(px(&out, 0, 0), B);
        assert_eq!(px(&out, 2, 0), R);
        assert_eq!(px(&out, 0, 1), Y);
        assert_eq!(px(&out, 2, 1), C);
    }

    #[test]
    fn orientation_3_rotates_180() {
        let out = apply(fixture(), Some(3));
        assert_eq!(out.dimensions(), (3, 2));
        // (0,0) becomes bottom-right
        assert_eq!(px(&out, 0, 0), Y);
        assert_eq!(px(&out, 2, 1), R);
    }

    #[test]
    fn orientation_4_flips_vertical() {
        let out = apply(fixture(), Some(4));
        assert_eq!(out.dimensions(), (3, 2));
        assert_eq!(px(&out, 0, 0), C);
        assert_eq!(px(&out, 2, 1), B);
    }

    #[test]
    fn orientation_6_rotates_90_cw() {
        // 90° CW takes a 3×2 image to 2×3.
        // Top-left R moves to top-right (pixel (1, 0)).
        // Bottom-left C moves to top-left (pixel (0, 0)).
        let out = apply(fixture(), Some(6));
        assert_eq!(out.dimensions(), (2, 3));
        assert_eq!(px(&out, 0, 0), C);
        assert_eq!(px(&out, 1, 0), R);
        assert_eq!(px(&out, 0, 2), Y);
        assert_eq!(px(&out, 1, 2), B);
    }

    #[test]
    fn orientation_8_rotates_90_ccw() {
        // 90° CCW takes a 3×2 image to 2×3.
        // Top-left R moves to bottom-left (pixel (0, 2)).
        let out = apply(fixture(), Some(8));
        assert_eq!(out.dimensions(), (2, 3));
        assert_eq!(px(&out, 0, 0), B);
        assert_eq!(px(&out, 0, 2), R);
        assert_eq!(px(&out, 1, 0), Y);
        assert_eq!(px(&out, 1, 2), C);
    }

    #[test]
    fn orientation_5_transpose() {
        // Transpose: mirror horizontally then rotate 90° CW.
        // Equivalent to reflection across the main diagonal.
        // Original (x, y) -> (y, x) in the output, so dims swap.
        let out = apply(fixture(), Some(5));
        assert_eq!(out.dimensions(), (2, 3));
        assert_eq!(px(&out, 0, 0), R);
        assert_eq!(px(&out, 1, 0), C);
        assert_eq!(px(&out, 0, 2), B);
        assert_eq!(px(&out, 1, 2), Y);
    }

    #[test]
    fn orientation_7_transverse() {
        // Transverse: reflection across the anti-diagonal.
        // Equivalent to mirror horizontally then rotate 90° CCW.
        let out = apply(fixture(), Some(7));
        assert_eq!(out.dimensions(), (2, 3));
        assert_eq!(px(&out, 0, 0), Y);
        assert_eq!(px(&out, 1, 0), B);
        assert_eq!(px(&out, 0, 2), C);
        assert_eq!(px(&out, 1, 2), R);
    }

    /// Build a 16×8 two-tone image: top half red, bottom half blue. The two
    /// halves are clearly distinguishable after rotation so we can tell a
    /// re-encoded JPEG actually reflects the rotation.
    fn tall_fixture() -> DynamicImage {
        let mut buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(16, 8);
        for y in 0..8 {
            for x in 0..16 {
                let color = if y < 4 { Rgb([255, 0, 0]) } else { Rgb([0, 0, 255]) };
                buf.put_pixel(x, y, color);
            }
        }
        DynamicImage::ImageRgb8(buf)
    }

    #[test]
    fn apply_and_reencode_passthrough_for_identity() {
        let img = tall_fixture();
        let original = encode_jpeg(&img, 92).unwrap();
        let (bytes, out) = apply_and_reencode(img, Some(1), original.clone()).unwrap();
        assert_eq!(bytes, original, "orientation=1 must return bytes unchanged");
        assert_eq!(out.dimensions(), (16, 8));
    }

    #[test]
    fn apply_and_reencode_passthrough_for_none() {
        let img = tall_fixture();
        let original = encode_jpeg(&img, 92).unwrap();
        let (bytes, _) = apply_and_reencode(img, None, original.clone()).unwrap();
        assert_eq!(bytes, original);
    }

    #[test]
    fn apply_and_reencode_rotates_and_reencodes() {
        let img = tall_fixture();
        let original = encode_jpeg(&img, 92).unwrap();
        let (bytes, out) = apply_and_reencode(img, Some(6), original.clone()).unwrap();

        // Image returned is rotated: 16×8 → 8×16
        assert_eq!(out.dimensions(), (8, 16));

        // Bytes must round-trip: decoding them yields the same rotated dims.
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.dimensions(), (8, 16));

        // And the rotated bytes are different from the pre-rotation bytes.
        assert_ne!(bytes, original);
    }

    #[test]
    fn label_omits_identity_and_unknown() {
        assert_eq!(label(Some(1)), None);
        assert_eq!(label(None), None);
        assert_eq!(label(Some(42)), None);
    }

    #[test]
    fn label_describes_rotation() {
        assert_eq!(label(Some(6)), Some("rotated 90° CW"));
        assert_eq!(label(Some(8)), Some("rotated 90° CCW"));
        assert_eq!(label(Some(3)), Some("rotated 180°"));
    }
}
