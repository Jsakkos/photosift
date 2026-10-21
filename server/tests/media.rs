mod common;

use std::path::{Path, PathBuf};

use common::{
    fake_raf, synthetic_jpeg, synthetic_jpeg_plain, synthetic_jpeg_with_exif, synthetic_rgb,
    ExifSpec, Pattern,
};
use photosift::media::{embedded, exif, phash, render, MediaError};

fn decode_dims(path: &Path) -> (u32, u32) {
    let bytes = std::fs::read(path).expect("read output");
    let mut d = zune_jpeg::JpegDecoder::new(&bytes);
    d.decode().expect("output decodes");
    let info = d.info().expect("info");
    (u32::from(info.width), u32::from(info.height))
}

fn within(actual: u32, expected: u32, tol: u32) -> bool {
    actual.abs_diff(expected) <= tol
}

// --- render ---------------------------------------------------------------

#[test]
fn orientation_6_rotates_portrait() {
    let dir = tempfile::tempdir().unwrap();
    let (p, t) = (dir.path().join("p.jpg"), dir.path().join("t.jpg"));
    let r = render::make_previews(&synthetic_jpeg(600, 400, 6), 6, &p, &t).unwrap();
    assert_eq!((r.display_w, r.display_h), (400, 600));
    assert_eq!(decode_dims(&p), (400, 600));
    assert_eq!(decode_dims(&t), (r.thumb_w, r.thumb_h));
}

#[test]
fn preview_and_thumb_long_edges() {
    let dir = tempfile::tempdir().unwrap();
    let (p, t) = (dir.path().join("p.jpg"), dir.path().join("t.jpg"));
    let r = render::make_previews(&synthetic_jpeg(6000, 4000, 1), 1, &p, &t).unwrap();
    assert_eq!((r.display_w, r.display_h), (6000, 4000));
    let (pw, ph) = decode_dims(&p);
    assert!(
        within(pw, 2560, 1) && within(ph, 1707, 1),
        "preview {pw}x{ph}"
    );
    let (tw, th) = decode_dims(&t);
    assert!(within(tw, 400, 1) && within(th, 267, 1), "thumb {tw}x{th}");
    assert_eq!((r.thumb_w, r.thumb_h), (tw, th));
    assert_eq!(r.thumb_rgb.len(), (tw * th * 3) as usize);
}

#[test]
fn small_images_are_not_upscaled_and_parent_dirs_are_created() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a/b/p.jpg");
    let t = dir.path().join("c/t.jpg");
    let r = render::make_previews(&synthetic_jpeg(300, 200, 1), 1, &p, &t).unwrap();
    assert_eq!(decode_dims(&p), (300, 200));
    assert_eq!((r.thumb_w, r.thumb_h), (300, 200));
}

#[test]
fn orientation_8_rotates_and_unknown_value_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let (p, t) = (dir.path().join("p.jpg"), dir.path().join("t.jpg"));
    let r = render::make_previews(&synthetic_jpeg(600, 400, 1), 8, &p, &t).unwrap();
    assert_eq!((r.display_w, r.display_h), (400, 600));
    // An out-of-range orientation means "no rotation", not a failed import.
    let r = render::make_previews(&synthetic_jpeg(600, 400, 1), 99, &p, &t).unwrap();
    assert_eq!((r.display_w, r.display_h), (600, 400));
}

#[test]
fn garbage_input_is_a_decode_error() {
    let dir = tempfile::tempdir().unwrap();
    let (p, t) = (dir.path().join("p.jpg"), dir.path().join("t.jpg"));
    let err = render::make_previews(b"not a jpeg", 1, &p, &t).unwrap_err();
    assert!(matches!(err, MediaError::Decode(_)), "{err:?}");
}

// --- phash ----------------------------------------------------------------

#[test]
fn phash_stable_and_discriminating() {
    let (w, h) = (400, 300);
    let base = synthetic_rgb(w, h, Pattern::Blocks, 1.0);
    let a = phash::compute(&base, w, h);
    assert_eq!(a, phash::compute(&base, w, h));

    let brighter = synthetic_rgb(w, h, Pattern::Blocks, 1.1);
    let d = phash::hamming(a, phash::compute(&brighter, w, h));
    assert!(d <= 4, "brightness +10% moved hash by {d}");

    let other = synthetic_rgb(w, h, Pattern::Stripes, 1.0);
    let d = phash::hamming(a, phash::compute(&other, w, h));
    assert!(d >= 20, "different pattern only {d} bits away");
}

#[test]
fn hamming_counts_bits_across_sign() {
    assert_eq!(phash::hamming(0, 0), 0);
    assert_eq!(phash::hamming(0, -1), 64);
    assert_eq!(phash::hamming(i64::MIN, 0), 1);
    assert_eq!(phash::hamming(0b1010, 0b0110), 2);
}

// --- embedded -------------------------------------------------------------

/// Filler with no 0xFF byte, so it can never fake a JPEG marker.
fn junk(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed) % 0xF0)
        .collect()
}

/// A valid JPEG of roughly `target` bytes, padded with COM segments whose payload is
/// all 0xFF (which the scan must skip over by length, not by byte pattern).
fn padded_jpeg(target: usize) -> Vec<u8> {
    let base = synthetic_jpeg_plain(64, 64);
    let mut out = base[..2].to_vec(); // SOI
    let mut remaining = target.saturating_sub(base.len());
    while remaining > 0 {
        let chunk = remaining.min(60_000);
        out.extend_from_slice(&[0xFF, 0xFE]);
        out.extend_from_slice(&((chunk + 2) as u16).to_be_bytes());
        out.extend(std::iter::repeat_n(0xFFu8, chunk));
        remaining -= chunk;
    }
    out.extend_from_slice(&base[2..]);
    out
}

#[test]
fn embedded_finds_largest() {
    let small = padded_jpeg(20_000);
    let big = padded_jpeg(60_000);
    let mut buf = junk(1000, 1);
    buf.extend_from_slice(&small);
    buf.extend(junk(5000, 2));
    buf.extend_from_slice(&big);
    buf.extend(junk(300, 3));
    let got = embedded::largest_jpeg(&buf, "bin").unwrap();
    assert_eq!(got, &big[..]);
}

#[test]
fn embedded_ignores_tiny_thumbnails_and_errors_when_none() {
    let tiny = synthetic_jpeg_plain(16, 16);
    assert!(tiny.len() < 10_000);
    let mut buf = junk(100, 1);
    buf.extend_from_slice(&tiny);
    assert!(matches!(
        embedded::largest_jpeg(&buf, "bin"),
        Err(MediaError::NoJpeg)
    ));
}

#[test]
fn embedded_truncated_jpeg_without_eoi_is_no_jpeg() {
    let mut jpeg = padded_jpeg(30_000);
    jpeg.truncate(jpeg.len() - 2); // drop FF D9
    assert!(matches!(
        embedded::largest_jpeg(&jpeg, "bin"),
        Err(MediaError::NoJpeg)
    ));
    // Cut mid-entropy-data as well.
    jpeg.truncate(jpeg.len() - 100);
    assert!(matches!(
        embedded::largest_jpeg(&jpeg, "bin"),
        Err(MediaError::NoJpeg)
    ));
}

#[test]
fn embedded_segment_length_overshooting_buffer_is_no_jpeg() {
    let mut buf = vec![0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF];
    buf.extend(junk(20_000, 4));
    assert!(matches!(
        embedded::largest_jpeg(&buf, "bin"),
        Err(MediaError::NoJpeg)
    ));
    // Length that would overflow naive offset arithmetic near the end of the buffer.
    let tail = [0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF, 0xFF, 0xFF];
    assert!(matches!(
        embedded::largest_jpeg(&tail, "bin"),
        Err(MediaError::NoJpeg)
    ));
}

#[test]
fn embedded_sos_without_eoi_is_no_jpeg() {
    // SOI, APP0 (len 2), SOS (len 2), then 20 KB of scan data and no FF D9.
    let mut buf = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02, 0xFF, 0xDA, 0x00, 0x02];
    buf.extend(junk(20_000, 5));
    assert!(matches!(
        embedded::largest_jpeg(&buf, "bin"),
        Err(MediaError::NoJpeg)
    ));
}

#[test]
fn embedded_scan_is_linear_on_many_eoi_less_headers() {
    // 64-byte cells, each a valid SOI+SOS header; no FF D9 anywhere. A scan that
    // restarts the EOI search per candidate would do ~65k searches of ~4 MB.
    let cell = |_: usize| {
        let mut c = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02, 0xFF, 0xDA, 0x00, 0x02];
        c.resize(64, 0x11);
        c
    };
    let buf: Vec<u8> = (0..65_536).flat_map(cell).collect();
    assert_eq!(buf.len(), 4 * 1024 * 1024);
    let start = std::time::Instant::now();
    let res = embedded::largest_jpeg(&buf, "bin");
    assert!(matches!(res, Err(MediaError::NoJpeg)));
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "scan took {:?}",
        start.elapsed()
    );
}

#[test]
fn embedded_nef_requires_tiff_magic() {
    let outer = padded_jpeg(30_000);
    let mut buf = junk(64, 9);
    buf.extend_from_slice(&outer);
    assert!(matches!(
        embedded::largest_jpeg(&buf, "nef"),
        Err(MediaError::InvalidContainer(_))
    ));
    // Same bytes behind a real TIFF header are accepted.
    let mut tiff = b"II*\0".to_vec();
    tiff.extend(junk(60, 9));
    tiff.extend_from_slice(&outer);
    assert_eq!(embedded::largest_jpeg(&tiff, "NEF").unwrap(), &outer[..]);
}

#[test]
fn embedded_raf_uses_header_offsets() {
    let jpeg = padded_jpeg(40_000);
    let raf = fake_raf(&jpeg);
    assert_eq!(embedded::largest_jpeg(&raf, "raf").unwrap(), &jpeg[..]);
    assert_eq!(embedded::largest_jpeg(&raf, "RAF").unwrap(), &jpeg[..]);
}

#[test]
fn embedded_raf_bad_magic_errors_and_bad_offsets_fall_back_to_scan() {
    let jpeg = padded_jpeg(40_000);
    let mut bad_magic = fake_raf(&jpeg);
    bad_magic[0] = b'X';
    assert!(matches!(
        embedded::largest_jpeg(&bad_magic, "raf"),
        Err(MediaError::InvalidContainer(_))
    ));

    let mut bad_off = fake_raf(&jpeg);
    bad_off[84..88].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(embedded::largest_jpeg(&bad_off, "raf").unwrap(), &jpeg[..]);
}

#[test]
fn embedded_jpeg_extension_returns_input_whole() {
    let jpeg = synthetic_jpeg_plain(200, 100);
    assert_eq!(embedded::largest_jpeg(&jpeg, "jpg").unwrap(), &jpeg[..]);
    assert_eq!(embedded::largest_jpeg(&jpeg, "JPEG").unwrap(), &jpeg[..]);
}

// --- exif -----------------------------------------------------------------

fn spec() -> ExifSpec<'static> {
    ExifSpec {
        orientation: Some(6),
        make: Some("NIKON CORPORATION"),
        model: Some("NIKON D750"),
        serial: Some("3012345"),
        taken: Some("2024:07:04 18:30:05"),
        pixel_dims: Some((6016, 4016)),
    }
}

#[test]
fn exif_reads_time_camera_orientation_dimensions() {
    let jpeg = synthetic_jpeg_with_exif(64, 48, Pattern::Blocks, &spec());
    let info = exif::read(&jpeg, Path::new("DSC_0001.JPG"));
    assert_eq!(
        info.capture_time,
        Some(
            chrono::NaiveDate::from_ymd_opt(2024, 7, 4)
                .unwrap()
                .and_hms_opt(18, 30, 5)
                .unwrap()
        )
    );
    assert_eq!(
        info.camera.as_deref(),
        Some("NIKON CORPORATION NIKON D750 #3012345")
    );
    assert_eq!(info.orientation, 6);
    assert_eq!((info.width, info.height), (6016, 4016));
}

#[test]
fn exif_camera_without_serial_is_make_and_model() {
    let mut s = spec();
    s.serial = None;
    let jpeg = synthetic_jpeg_with_exif(64, 48, Pattern::Blocks, &s);
    let info = exif::read(&jpeg, Path::new("a.jpg"));
    assert_eq!(info.camera.as_deref(), Some("NIKON CORPORATION NIKON D750"));
}

#[test]
fn exif_missing_everything_is_none_not_error() {
    let info = exif::read(&synthetic_jpeg_plain(64, 48), Path::new("a.jpg"));
    assert_eq!(info.capture_time, None);
    assert_eq!(info.camera, None);
    assert_eq!(info.orientation, 1);
    let info = exif::read(b"definitely not an image", Path::new("a.nef"));
    assert_eq!(info.capture_time, None);
    assert_eq!(info.orientation, 1);
}

#[test]
fn exif_reads_raf_from_embedded_jpeg_and_from_a_prefix() {
    let jpeg = synthetic_jpeg_with_exif(64, 48, Pattern::Blocks, &spec());
    let raf = fake_raf(&jpeg);
    let full = exif::read(&raf, Path::new("DSCF0001.RAF"));
    assert_eq!(
        full.camera.as_deref(),
        Some("NIKON CORPORATION NIKON D750 #3012345")
    );
    assert!(full.capture_time.is_some());
    // Scanning passes only a leading slice of the file; the JPEG's EXIF sits at its start.
    let prefix = &raf[..160 + 400.min(jpeg.len())];
    let part = exif::read(prefix, Path::new("DSCF0001.RAF"));
    assert_eq!(part.capture_time, full.capture_time);
    assert_eq!(part.camera, full.camera);
}

// --- golden (real cameras; fixture lives outside git) ----------------------

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("read golden dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
#[ignore = "needs PHOTOSIFT_GOLDEN_DIR (real RAF/NEF fixture, built in Task 6)"]
fn golden_raf_and_nef() {
    // Panic rather than skip: an --ignored run that checked nothing must not look green.
    let dir = std::env::var_os("PHOTOSIFT_GOLDEN_DIR")
        .expect("set PHOTOSIFT_GOLDEN_DIR; see tests/golden/README.md");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    let mut checked = 0;
    for path in files {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "raf" && ext != "nef" {
            continue;
        }
        let raw = std::fs::read(&path).unwrap();
        let jpeg = embedded::largest_jpeg(&raw, &ext).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        assert!(
            jpeg.len() >= 1_000_000,
            "{path:?}: embedded jpeg only {} bytes",
            jpeg.len()
        );
        let info = exif::read(&raw, &path);
        assert!(info.capture_time.is_some(), "{path:?}: no capture time");
        if ext == "raf" {
            assert_eq!((info.width, info.height), (4416, 2944), "{path:?}");
        }
        checked += 1;
    }
    assert!(checked > 0, "golden dir held no RAF/NEF files");
}
