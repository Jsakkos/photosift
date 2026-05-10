use crate::pipeline::embedded;
use exif::{Context, Exif, In, Reader, Tag, Value};
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExifData {
    pub capture_time: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: Option<i32>,
    /// XMP-compatible rating (0-5), read from EXIF Rating tag (0x4746)
    /// or derived from RatingPercent (0x4749) as a fallback.
    pub rating: Option<i32>,
}

pub fn extract_exif(path: &Path) -> Result<ExifData, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    // RAF is not a TIFF/JPEG container, so kamadak-exif's
    // read_from_container can't parse it directly. Pull the largest
    // embedded JPEG out of the RAF first — that JPEG carries the
    // standard EXIF block written by the camera — and parse from those
    // bytes. Other containers (NEF/JPEG/TIFF) take the original path.
    let parsed = match ext.as_deref() {
        Some("raf") => extract_exif_from_raf(path),
        _ => extract_exif_from_container(path),
    };

    let mut data = match parsed {
        Ok(d) => d,
        Err(e) => {
            // Both EXIF strategies failed (corrupt RAF, JPEG with no
            // EXIF, etc.). Fall back to a stub so callers still get
            // an mtime-derived capture_time.
            log::debug!("extract_exif: {} → no EXIF ({}); will try mtime", path.display(), e);
            empty_exif_data()
        }
    };

    if data.capture_time.is_none() {
        data.capture_time = mtime_as_exif_string(path);
    }

    Ok(data)
}

fn extract_exif_from_container(path: &Path) -> Result<ExifData, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let exif = Reader::new()
        .read_from_container(&mut BufReader::new(&file))
        .map_err(|e| e.to_string())?;
    Ok(parse_exif_fields(&exif))
}

fn extract_exif_from_raf(path: &Path) -> Result<ExifData, String> {
    let jpegs = embedded::extract_all_jpegs(path).map_err(|e| e.to_string())?;
    let bytes = &jpegs[0].bytes;
    let exif = Reader::new()
        .read_from_container(&mut BufReader::new(Cursor::new(bytes.as_slice())))
        .map_err(|e| e.to_string())?;
    Ok(parse_exif_fields(&exif))
}

fn parse_exif_fields(exif: &Exif) -> ExifData {
    let capture_time = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .map(|f| f.display_value().to_string());

    let camera_model = exif
        .get_field(Tag::Model, In::PRIMARY)
        .map(|f| f.display_value().to_string().trim_matches('"').to_string());

    let lens = exif
        .get_field(Tag::LensModel, In::PRIMARY)
        .map(|f| f.display_value().to_string().trim_matches('"').to_string());

    let focal_length = exif.get_field(Tag::FocalLength, In::PRIMARY).and_then(|f| {
        if let Value::Rational(ref v) = f.value {
            v.first().map(|r| r.to_f64())
        } else {
            None
        }
    });

    let aperture = exif.get_field(Tag::FNumber, In::PRIMARY).and_then(|f| {
        if let Value::Rational(ref v) = f.value {
            v.first().map(|r| r.to_f64())
        } else {
            None
        }
    });

    let shutter_speed = exif
        .get_field(Tag::ExposureTime, In::PRIMARY)
        .map(|f| f.display_value().to_string());

    let iso = exif
        .get_field(Tag::PhotographicSensitivity, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let width = exif
        .get_field(Tag::PixelXDimension, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::ImageWidth, In::PRIMARY))
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let height = exif
        .get_field(Tag::PixelYDimension, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::ImageLength, In::PRIMARY))
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    // Rating (0x4746) preferred; fall back to RatingPercent (0x4749) mapped per
    // Windows/Microsoft convention: 0, 25, 50, 75, 99/100 → 0..5. Both live in
    // the primary TIFF IFD.
    let rating = exif
        .get_field(Tag(Context::Tiff, 0x4746), In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32))
        .or_else(|| {
            exif.get_field(Tag(Context::Tiff, 0x4749), In::PRIMARY)
                .and_then(|f| f.value.get_uint(0).map(|v| match v {
                    0 => 0,
                    1..=24 => 1,
                    25..=49 => 2,
                    50..=74 => 3,
                    75..=98 => 4,
                    _ => 5,
                }))
        })
        .map(|r| r.clamp(0, 5));

    ExifData {
        capture_time,
        camera_model,
        lens,
        focal_length,
        aperture,
        shutter_speed,
        iso,
        width,
        height,
        orientation,
        rating,
    }
}

fn empty_exif_data() -> ExifData {
    ExifData {
        capture_time: None,
        camera_model: None,
        lens: None,
        focal_length: None,
        aperture: None,
        shutter_speed: None,
        iso: None,
        width: None,
        height: None,
        orientation: None,
        rating: None,
    }
}

/// Format the file's mtime as an EXIF-shaped capture_time string
/// (`YYYY-MM-DD HH:MM:SS`). Used as a last-resort date so files without
/// any EXIF still land in some bucket of the date-grouped browser.
fn mtime_as_exif_string(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = modified.into();
    Some(dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Extract a small thumbnail JPEG. For NEF/JPEG/TIFF this reads the
/// IFD1 thumbnail (~10–20 KB at the start of the file) without pulling
/// the entire 30–50 MB raw into memory. For RAF — which has no IFD1
/// chain — return the smallest embedded JPEG, which is typically the
/// camera's TIFF-thumb-equivalent at a similar size.
///
/// On a slow SD card the IFD1 path is the difference between ~1 s/file
/// and ~30 ms/file. Returns `Err` for files where neither shortcut
/// applies — caller should fall back to the largest-JPEG path.
pub fn extract_ifd1_thumbnail(path: &Path) -> Result<Vec<u8>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    if ext.as_deref() == Some("raf") {
        let mut jpegs = embedded::extract_all_jpegs(path).map_err(|e| e.to_string())?;
        let last = jpegs.pop().ok_or_else(|| "no embedded JPEG in RAF".to_string())?;
        return Ok(last.bytes);
    }

    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file);
    let exif = Reader::new()
        .read_from_container(&mut reader)
        .map_err(|e| e.to_string())?;

    // EXIF tag 0x0201 / 0x0202 in IFD1 hold the offset/length of the
    // small thumbnail JPEG. kamadak-exif models IFD1 as `In::THUMBNAIL`.
    let offset = exif
        .get_field(Tag(Context::Tiff, 0x0201), In::THUMBNAIL)
        .and_then(|f| f.value.get_uint(0))
        .ok_or_else(|| "missing IFD1 JPEGInterchangeFormat".to_string())?;
    let length = exif
        .get_field(Tag(Context::Tiff, 0x0202), In::THUMBNAIL)
        .and_then(|f| f.value.get_uint(0))
        .ok_or_else(|| "missing IFD1 JPEGInterchangeFormatLength".to_string())?;

    if length == 0 || length > 1_000_000 {
        return Err(format!(
            "suspicious IFD1 thumbnail length: {} bytes",
            length
        ));
    }

    reader
        .seek(SeekFrom::Start(offset as u64))
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; length as usize];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;

    // Sanity check: must start with the JPEG SOI marker.
    if buf.len() < 2 || buf[0] != 0xFF || buf[1] != 0xD8 {
        return Err("IFD1 thumbnail bytes do not start with JPEG SOI".to_string());
    }

    Ok(buf)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Build a minimal valid JPEG that kamadak-exif can parse and that
    /// carries `DateTimeOriginal` + `Model`. Layout:
    ///
    /// ```
    /// FF D8                     SOI
    /// FF E1 LL LL Exif\0\0 ...  APP1 + EXIF (TIFF block)
    /// FF FE LL LL [bytes...]    COM marker padding (to push >10KB)
    /// FF D9                     EOI
    /// ```
    ///
    /// Hand-rolled because we don't ship a real fixture file and the
    /// `image` crate doesn't write EXIF. The TIFF block has IFD0 with
    /// Model + ExifIFDPointer, and a child EXIF SubIFD with
    /// DateTimeOriginal — the smallest set that exercises the fields
    /// PhotoSift's pipeline cares about.
    pub(crate) fn make_jpeg_with_exif(date_time_original: &str, camera_model: &str) -> Vec<u8> {
        // The string fields stored in EXIF are null-terminated ASCII
        // and must round-trip verbatim, so include the trailing \0.
        let dto = format!("{}\0", date_time_original);
        let model = format!("{}\0", camera_model);
        let dto_bytes = dto.as_bytes();
        let model_bytes = model.as_bytes();
        let dto_len = dto_bytes.len() as u32;
        let model_len = model_bytes.len() as u32;

        // Layout of the TIFF block (after `Exif\0\0`):
        //   offset 0:  TIFF header                              (8 bytes)
        //   offset 8:  IFD0 = 2 + (2 entries * 12) + 4 = 30 bytes
        //   offset 38: EXIF SubIFD = 2 + 12 + 4 = 18 bytes
        //   offset 56: Model string
        //   offset 56+model_len: DateTimeOriginal string
        let ifd0_offset: u32 = 8;
        let ifd0_size: u32 = 2 + 2 * 12 + 4;
        let exif_ifd_offset: u32 = ifd0_offset + ifd0_size;
        let exif_ifd_size: u32 = 2 + 12 + 4;
        let model_str_offset: u32 = exif_ifd_offset + exif_ifd_size;
        let dto_str_offset: u32 = model_str_offset + model_len;

        let mut tiff = Vec::new();
        // TIFF header: little-endian, magic 42, IFD0 offset.
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&0x002Au16.to_le_bytes());
        tiff.extend_from_slice(&ifd0_offset.to_le_bytes());

        // IFD0: 2 entries.
        tiff.extend_from_slice(&2u16.to_le_bytes());
        // Entry 1: Model (0x0110), type=ASCII, count=model_len, value=offset.
        tiff.extend_from_slice(&0x0110u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&model_len.to_le_bytes());
        tiff.extend_from_slice(&model_str_offset.to_le_bytes());
        // Entry 2: ExifIFDPointer (0x8769), type=LONG, count=1.
        tiff.extend_from_slice(&0x8769u16.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&exif_ifd_offset.to_le_bytes());
        // Next IFD pointer = 0.
        tiff.extend_from_slice(&0u32.to_le_bytes());

        // EXIF SubIFD: 1 entry.
        tiff.extend_from_slice(&1u16.to_le_bytes());
        // Entry: DateTimeOriginal (0x9003), type=ASCII, count, value.
        tiff.extend_from_slice(&0x9003u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&dto_len.to_le_bytes());
        tiff.extend_from_slice(&dto_str_offset.to_le_bytes());
        // Next IFD pointer = 0.
        tiff.extend_from_slice(&0u32.to_le_bytes());

        // String pool.
        tiff.extend_from_slice(model_bytes);
        tiff.extend_from_slice(dto_bytes);

        // Wrap into APP1 segment. Length field includes itself (2 bytes)
        // plus "Exif\0\0" (6 bytes) plus the TIFF block.
        let app1_len = (2 + 6 + tiff.len()) as u16;

        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
        jpeg.extend_from_slice(&[0xFF, 0xE1]); // APP1 marker
        jpeg.extend_from_slice(&app1_len.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);

        // Pad with COM (FFFE) markers so the embedded-JPEG scanner's
        // 10 KB minimum-blob threshold doesn't reject us. One COM marker
        // can carry up to 65 535 - 2 = 65 533 bytes of payload.
        let pad_bytes = 12_000usize;
        let com_len = (pad_bytes + 2) as u16;
        jpeg.extend_from_slice(&[0xFF, 0xFE]);
        jpeg.extend_from_slice(&com_len.to_be_bytes());
        jpeg.resize(jpeg.len() + pad_bytes, 0u8);

        jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI
        jpeg
    }

    /// Build a minimal "RAF" file: the FUJIFILMCCD-RAW magic (with a
    /// version-string tail) followed by the JPEG bytes from
    /// `make_jpeg_with_exif`. The byte-by-byte JPEG marker scanner in
    /// `pipeline::embedded` finds the JPEG anywhere in the file, so the
    /// magic prefix is the only structural requirement.
    pub(crate) fn make_synthetic_raf(date_time_original: &str, camera_model: &str) -> Vec<u8> {
        let mut raf = Vec::new();
        raf.extend_from_slice(b"FUJIFILMCCD-RAW ");
        // 16 bytes of arbitrary "version" bytes — the embedded scanner
        // skips ahead until it finds FF D8 FF <valid_marker>.
        raf.extend_from_slice(b"0201FF383501020");
        raf.push(0u8);
        let jpeg = make_jpeg_with_exif(date_time_original, camera_model);
        raf.extend_from_slice(&jpeg);
        raf
    }

    #[test]
    fn synthetic_jpeg_round_trips_through_extract_exif() {
        // Sanity check that our hand-rolled JPEG fixture is in fact
        // parseable by kamadak-exif. If this test fails the *other*
        // RAF/EXIF tests below will fail without revealing whether the
        // bug is in our test fixture or in extract_exif itself.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jpg");
        std::fs::write(
            &path,
            make_jpeg_with_exif("2024:01:15 14:30:00", "FUJIFILM X100VI"),
        )
        .unwrap();

        let data = extract_exif(&path).expect("extract_exif should succeed on synthetic JPEG");
        assert!(
            data.capture_time.is_some(),
            "capture_time should be populated"
        );
        assert!(
            data.camera_model
                .as_deref()
                .map(|s| s.contains("X100VI"))
                .unwrap_or(false),
            "camera_model should contain 'X100VI', got {:?}",
            data.camera_model
        );
    }

    #[test]
    fn raf_with_embedded_jpeg_extracts_capture_time() {
        // The bug we're fixing: kamadak-exif can't parse the RAF
        // container directly, so extract_exif's RAF branch falls back
        // to the embedded JPEG and parses EXIF from those bytes.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("DSCF1234.RAF");
        std::fs::write(
            &path,
            make_synthetic_raf("2024:08:02 09:15:00", "FUJIFILM X-T5"),
        )
        .unwrap();

        let data = extract_exif(&path).expect("extract_exif should succeed on synthetic RAF");
        assert!(
            data.capture_time.is_some(),
            "RAF capture_time must come through embedded-JPEG dispatch (got {:?})",
            data.capture_time
        );
        assert!(
            data.camera_model
                .as_deref()
                .map(|s| s.contains("X-T5"))
                .unwrap_or(false),
            "RAF camera_model must come through embedded-JPEG dispatch (got {:?})",
            data.camera_model
        );
    }

    #[test]
    fn raf_extension_is_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let upper = dir.path().join("UPPER.RAF");
        let lower = dir.path().join("lower.raf");
        std::fs::write(&upper, make_synthetic_raf("2024:08:02 09:15:00", "X-T5")).unwrap();
        std::fs::write(&lower, make_synthetic_raf("2024:08:02 09:16:00", "X-T5")).unwrap();

        for p in [&upper, &lower] {
            let data = extract_exif(p).expect("extract_exif should succeed");
            assert!(
                data.capture_time.is_some(),
                "{:?}: capture_time should populate regardless of extension case",
                p,
            );
        }
    }

    #[test]
    fn jpeg_extract_exif_does_not_regress_after_refactor() {
        // Regression guard: the parse_exif_fields refactor must not
        // change behavior for the most common path (JPEG container).
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("regular.jpg");
        std::fs::write(
            &path,
            make_jpeg_with_exif("2024:01:15 14:30:00", "NIKON D750"),
        )
        .unwrap();

        let data = extract_exif(&path).unwrap();
        assert!(data.capture_time.is_some());
        assert!(data.camera_model.is_some());
    }

    #[test]
    fn missing_exif_falls_back_to_mtime() {
        // Create a fake .raf whose first 16 bytes don't match the
        // RAF magic — the RAF dispatch will return an error, then
        // mtime fallback should kick in.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("fake.raf");
        std::fs::write(&path, b"not a real RAF file at all").unwrap();

        let data = extract_exif(&path).expect("extract_exif should not error");
        assert!(
            data.capture_time.is_some(),
            "mtime fallback should populate capture_time"
        );
        assert!(data.camera_model.is_none());
    }

    #[test]
    fn jpeg_with_no_exif_falls_back_to_mtime() {
        // A degenerate "JPEG" — kamadak-exif refuses files without
        // valid EXIF, so the container path errors and mtime kicks in.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nope.jpg");
        std::fs::write(&path, b"\xff\xd8\xff\xd9").unwrap();

        let data = extract_exif(&path).expect("extract_exif should not error");
        assert!(data.capture_time.is_some());
    }

    #[test]
    fn raf_with_no_exif_in_embedded_jpeg_falls_back_to_mtime() {
        // RAF that has the right magic but a JPEG body with no EXIF.
        // The RAF dispatch finds the JPEG, kamadak-exif fails to find
        // an APP1/EXIF segment in it, and mtime kicks in.
        let mut raf = Vec::new();
        raf.extend_from_slice(b"FUJIFILMCCD-RAW \0");
        raf.extend_from_slice(&[0xFF, 0xD8]); // SOI
        // Fill with a COM marker for size, then EOI.
        raf.extend_from_slice(&[0xFF, 0xFE]);
        let pad = 12_000usize;
        raf.extend_from_slice(&((pad + 2) as u16).to_be_bytes());
        raf.resize(raf.len() + pad, 0u8);
        raf.extend_from_slice(&[0xFF, 0xD9]); // EOI

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("noexif.RAF");
        std::fs::write(&path, &raf).unwrap();

        let data = extract_exif(&path).expect("extract_exif should not error");
        assert!(
            data.capture_time.is_some(),
            "lone-RAF mtime fallback should still produce a date"
        );
    }

    #[test]
    fn extract_ifd1_thumbnail_for_raf_returns_smallest_jpeg() {
        // RAF with two embedded JPEGs of different sizes; the helper
        // should hand back the *smallest* (i.e., the thumbnail-shaped
        // one), which is what scan-thumb building wants.
        let small = make_jpeg_with_exif("2024:01:15 14:30:00", "X-T5");
        // A "larger" JPEG is just our synthetic JPEG with extra COM
        // padding to ensure size-ordering after sort-largest-first.
        let mut larger = small.clone();
        // Insert a fat COM marker right before EOI (last 2 bytes are FFD9).
        let eoi_pos = larger.len() - 2;
        let extra_pad = 30_000usize;
        let com_len = (extra_pad + 2) as u16;
        let mut com = vec![0xFFu8, 0xFE];
        com.extend_from_slice(&com_len.to_be_bytes());
        com.resize(com.len() + extra_pad, 0u8);
        larger.splice(eoi_pos..eoi_pos, com);

        let mut raf = Vec::new();
        raf.extend_from_slice(b"FUJIFILMCCD-RAW \0");
        raf.extend_from_slice(&larger);
        raf.extend_from_slice(&small);

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("twojpegs.RAF");
        std::fs::write(&path, &raf).unwrap();

        let bytes = extract_ifd1_thumbnail(&path).expect("RAF thumbnail dispatch should succeed");
        // Sanity: bytes start with SOI and we got the smaller blob
        // (within ~10% of `small.len()`, definitely smaller than
        // `larger.len()`).
        assert_eq!(&bytes[0..2], &[0xFF, 0xD8]);
        assert!(
            bytes.len() < larger.len(),
            "expected thumbnail (smaller blob), got {} bytes",
            bytes.len()
        );
    }
}
