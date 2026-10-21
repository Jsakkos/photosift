//! Synthetic JPEG/RAF/EXIF builders shared by the integration tests and the
//! `make-synthetic-card` binary. Nothing binary is committed; every image is
//! built here.

#[derive(Clone, Copy)]
pub enum Pattern {
    /// Diagonal gradient with a few solid blocks: smooth, low-frequency content.
    Blocks,
    /// Horizontal bands: structurally unrelated to `Blocks`.
    Stripes,
}

/// RGB8 pixels for `pattern`; `gain` scales brightness (1.1 = +10%).
pub fn synthetic_rgb(w: u32, h: u32, pattern: Pattern, gain: f32) -> Vec<u8> {
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let rgb = match pattern {
                Pattern::Blocks => {
                    let mut v = [
                        60.0 + 120.0 * fx,
                        40.0 + 100.0 * fy,
                        90.0 + 60.0 * (fx + fy) / 2.0,
                    ];
                    if (0.1..0.4).contains(&fx) && (0.15..0.5).contains(&fy) {
                        v = [220.0, 200.0, 40.0];
                    }
                    if (0.55..0.9).contains(&fx) && (0.5..0.85).contains(&fy) {
                        v = [20.0, 30.0, 25.0];
                    }
                    v
                }
                Pattern::Stripes => {
                    let on = ((fy * 8.0) as u32).is_multiple_of(2);
                    if on {
                        [230.0, 230.0, 235.0]
                    } else {
                        [25.0, 20.0, 30.0]
                    }
                }
            };
            for c in rgb {
                px.push((c * gain).clamp(0.0, 255.0) as u8);
            }
        }
    }
    px
}

pub enum Tag {
    Ascii(u16, String),
    Short(u16, u16),
    Long(u16, u32),
}

impl Tag {
    fn id(&self) -> u16 {
        match self {
            Tag::Ascii(t, _) | Tag::Short(t, _) | Tag::Long(t, _) => *t,
        }
    }
}

/// Little-endian TIFF with an IFD0 and (optionally) an Exif sub-IFD, enough
/// for kamadak-exif to parse. The ExifIFDPointer entry is added automatically.
pub fn tiff_bytes(mut ifd0: Vec<Tag>, mut exif: Vec<Tag>) -> Vec<u8> {
    if !exif.is_empty() {
        ifd0.push(Tag::Long(0x8769, 0)); // patched below
    }
    ifd0.sort_by_key(Tag::id);
    exif.sort_by_key(Tag::id);
    let ifd_len = |n: usize| 2 + 12 * n + 4;
    let ifd0_off = 8u32;
    let exif_off = ifd0_off + ifd_len(ifd0.len()) as u32;
    let data_start = exif_off
        + if exif.is_empty() {
            0
        } else {
            ifd_len(exif.len()) as u32
        };

    let mut data: Vec<u8> = Vec::new();
    let mut write_ifd = |tags: &[Tag], patch_exif: bool| -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        for t in tags {
            let (id, ty, count, val): (u16, u16, u32, [u8; 4]) = match t {
                Tag::Short(id, v) => {
                    let mut b = [0u8; 4];
                    b[..2].copy_from_slice(&v.to_le_bytes());
                    (*id, 3, 1, b)
                }
                Tag::Long(id, v) => {
                    let v = if patch_exif && *id == 0x8769 {
                        exif_off
                    } else {
                        *v
                    };
                    (*id, 4, 1, v.to_le_bytes())
                }
                Tag::Ascii(id, s) => {
                    let mut bytes = s.as_bytes().to_vec();
                    bytes.push(0);
                    let count = bytes.len() as u32;
                    if bytes.len() <= 4 {
                        let mut b = [0u8; 4];
                        b[..bytes.len()].copy_from_slice(&bytes);
                        (*id, 2, count, b)
                    } else {
                        let off = data_start + data.len() as u32;
                        data.extend_from_slice(&bytes);
                        if data.len() % 2 == 1 {
                            data.push(0);
                        }
                        (*id, 2, count, off.to_le_bytes())
                    }
                }
            };
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&ty.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&val);
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    };
    let ifd0_bytes = write_ifd(&ifd0, true);
    let exif_bytes = if exif.is_empty() {
        Vec::new()
    } else {
        write_ifd(&exif, false)
    };

    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&ifd0_off.to_le_bytes());
    tiff.extend_from_slice(&ifd0_bytes);
    tiff.extend_from_slice(&exif_bytes);
    tiff.extend_from_slice(&data);
    tiff
}

#[derive(Default)]
pub struct ExifSpec<'a> {
    pub orientation: Option<u16>,
    pub make: Option<&'a str>,
    pub model: Option<&'a str>,
    pub serial: Option<&'a str>,
    /// `YYYY:MM:DD HH:MM:SS`
    pub taken: Option<&'a str>,
    pub pixel_dims: Option<(u32, u32)>,
}

fn exif_segment(spec: &ExifSpec<'_>) -> Vec<u8> {
    let mut ifd0 = Vec::new();
    if let Some(o) = spec.orientation {
        ifd0.push(Tag::Short(0x0112, o));
    }
    if let Some(m) = spec.make {
        ifd0.push(Tag::Ascii(0x010F, m.into()));
    }
    if let Some(m) = spec.model {
        ifd0.push(Tag::Ascii(0x0110, m.into()));
    }
    let mut exif = Vec::new();
    if let Some(t) = spec.taken {
        exif.push(Tag::Ascii(0x9003, t.into()));
    }
    if let Some(s) = spec.serial {
        exif.push(Tag::Ascii(0xA431, s.into()));
    }
    if let Some((w, h)) = spec.pixel_dims {
        exif.push(Tag::Long(0xA002, w));
        exif.push(Tag::Long(0xA003, h));
    }
    let mut seg = b"Exif\0\0".to_vec();
    seg.extend_from_slice(&tiff_bytes(ifd0, exif));
    seg
}

/// A real baseline JPEG carrying the EXIF described by `spec`.
pub fn synthetic_jpeg_with_exif(w: u32, h: u32, pattern: Pattern, spec: &ExifSpec<'_>) -> Vec<u8> {
    let rgb = synthetic_rgb(w, h, pattern, 1.0);
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, 90);
    enc.add_app_segment(1, &exif_segment(spec)).expect("app1");
    enc.encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)
        .expect("encode");
    out
}

/// A JPEG with only an orientation tag.
pub fn synthetic_jpeg(w: u32, h: u32, orientation: u16) -> Vec<u8> {
    synthetic_jpeg_with_exif(
        w,
        h,
        Pattern::Blocks,
        &ExifSpec {
            orientation: Some(orientation),
            ..Default::default()
        },
    )
}

/// A JPEG with no EXIF at all.
pub fn synthetic_jpeg_plain(w: u32, h: u32) -> Vec<u8> {
    let rgb = synthetic_rgb(w, h, Pattern::Blocks, 1.0);
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 90)
        .encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)
        .expect("encode");
    out
}

/// A Fujifilm-RAF-shaped file: magic, big-endian JPEG offset/length at 84/88,
/// then the JPEG at offset 160 followed by trailing "sensor" bytes.
pub fn fake_raf(jpeg: &[u8]) -> Vec<u8> {
    let mut raf = vec![0u8; 160];
    raf[..16].copy_from_slice(b"FUJIFILMCCD-RAW ");
    raf[84..88].copy_from_slice(&160u32.to_be_bytes());
    raf[88..92].copy_from_slice(&(jpeg.len() as u32).to_be_bytes());
    raf.extend_from_slice(jpeg);
    raf.extend(std::iter::repeat_n(0xA5u8, 4096));
    raf
}
