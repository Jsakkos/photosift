//! 64-bit perceptual hash: 32x32 grayscale, DCT, 8x8 low-frequency block,
//! each bit set where a coefficient exceeds the median of the AC terms.

use std::sync::OnceLock;

use fast_image_resize::{
    images::{Image, ImageRef},
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
};

const N: usize = 32;
const LOW: usize = 8;

/// Orthonormal DCT-II basis, built once.
fn dct_matrix() -> &'static [[f64; N]; N] {
    static M: OnceLock<[[f64; N]; N]> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = [[0.0; N]; N];
        for (u, row) in m.iter_mut().enumerate() {
            for (x, v) in row.iter_mut().enumerate() {
                let c = if u == 0 { 0.5f64.sqrt() } else { 1.0 };
                *v = ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / (2 * N) as f64).cos()
                    * c
                    * 0.5;
            }
        }
        m
    })
}

/// Hash of an RGB8 image (`w*h*3` bytes), returned as the u64 bit pattern in
/// an i64 so it stores directly in SQLite. Panics if the buffer does not match
/// the stated size or is empty: that is a caller bug, not bad input data.
pub fn compute(thumb_rgb: &[u8], w: u32, h: u32) -> i64 {
    assert!(w > 0 && h > 0, "phash of an empty image");
    assert_eq!(
        thumb_rgb.len(),
        w as usize * h as usize * 3,
        "rgb buffer does not match {w}x{h}"
    );

    let src = ImageRef::new(w, h, thumb_rgb, PixelType::U8x3).expect("buffer size checked above");
    let mut small = Image::new(N as u32, N as u32, PixelType::U8x3);
    Resizer::new()
        .resize(
            &src,
            &mut small,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)),
        )
        .expect("same pixel type, non-empty images");

    let gray: Vec<f64> = small
        .buffer()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
        .collect();

    // D * P * D^T, computing only the LOW x LOW corner.
    let m = dct_matrix();
    let mut tmp = [[0.0f64; N]; LOW];
    for (u, row) in tmp.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            *cell = (0..N).map(|y| m[u][y] * gray[y * N + x]).sum();
        }
    }
    let mut block = [0.0f64; LOW * LOW];
    for u in 0..LOW {
        for v in 0..LOW {
            block[u * LOW + v] = (0..N).map(|x| tmp[u][x] * m[v][x]).sum();
        }
    }

    // Median excludes the DC term: it carries overall brightness, which would skew it.
    let mut ac = block[1..].to_vec();
    ac.sort_by(f64::total_cmp);
    let median = ac[ac.len() / 2];
    block
        .iter()
        .fold(0u64, |bits, &v| (bits << 1) | u64::from(v > median)) as i64
}

pub fn hamming(a: i64, b: i64) -> u32 {
    (a ^ b).count_ones()
}
