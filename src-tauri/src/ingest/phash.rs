use image::imageops::FilterType;
use image::DynamicImage;

/// Hand-rolled DCT perceptual hash.
/// 1. Resize to 32x32 grayscale
/// 2. Apply 2D DCT
/// 3. Take the top-left 8x8 low-frequency block (excluding DC)
/// 4. Threshold against the median → 64-bit hash
pub fn compute_phash(img: &DynamicImage) -> [u8; 8] {
    let small = img
        .resize_exact(32, 32, FilterType::Triangle)
        .to_luma8();
    let pixels: Vec<f64> = small.pixels().map(|p| p.0[0] as f64).collect();

    let mut dct = vec![0.0f64; 32 * 32];
    for u in 0..32 {
        for v in 0..32 {
            let mut sum = 0.0;
            for x in 0..32 {
                for y in 0..32 {
                    let px = pixels[x * 32 + y];
                    sum += px
                        * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 64.0).cos()
                        * ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI / 64.0).cos();
                }
            }
            let cu = if u == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
            let cv = if v == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
            dct[u * 32 + v] = sum * cu * cv * 0.25;
        }
    }

    // Extract 8x8 low-freq block (64 coefficients for 64-bit hash)
    let mut values = Vec::with_capacity(64);
    for u in 0..8 {
        for v in 0..8 {
            values.push(dct[u * 32 + v]);
        }
    }

    // Median threshold
    let mut sorted = values.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];

    let mut hash = [0u8; 8];
    for (i, &val) in values.iter().enumerate() {
        if val > median {
            hash[i / 8] |= 1 << (7 - (i % 8));
        }
    }

    hash
}

pub fn hamming_distance(a: &[u8; 8], b: &[u8; 8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phash_produces_8_bytes() {
        let img = DynamicImage::new_rgb8(100, 100);
        let hash = compute_phash(&img);
        assert_eq!(hash.len(), 8);
    }

    #[test]
    fn test_phash_stable() {
        let img = DynamicImage::new_rgb8(200, 200);
        let h1 = compute_phash(&img);
        let h2 = compute_phash(&img);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hamming_distance_zero() {
        let a = [0xFF; 8];
        assert_eq!(hamming_distance(&a, &a), 0);
    }

    #[test]
    fn test_hamming_distance_max() {
        let a = [0x00; 8];
        let b = [0xFF; 8];
        assert_eq!(hamming_distance(&a, &b), 64);
    }
}
