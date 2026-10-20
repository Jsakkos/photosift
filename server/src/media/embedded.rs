//! Finds the largest JPEG inside a camera file held in memory. The import job
//! reads each file once and reuses that buffer for hashing, so nothing here
//! touches the filesystem.

use super::MediaError;

/// 16-byte magic at the head of every Fujifilm RAF.
const RAF_MAGIC: &[u8; 16] = b"FUJIFILMCCD-RAW ";
/// Blobs at or below this size are EXIF thumbnails, not previews.
const MIN_PREVIEW_BYTES: usize = 10_000;

/// The embedded preview for a RAW (`nef`/`raf`), the whole buffer for a JPEG,
/// or the largest JPEG found by marker scan for any other extension.
pub fn largest_jpeg<'a>(raw: &'a [u8], ext: &str) -> Result<&'a [u8], MediaError> {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Ok(raw),
        "nef" => {
            if !is_tiff(raw) {
                return Err(MediaError::InvalidContainer("NEF"));
            }
            scan_largest(raw)
        }
        "raf" => {
            if !raw.starts_with(RAF_MAGIC) {
                return Err(MediaError::InvalidContainer("RAF"));
            }
            // The header names the camera JPEG exactly; a bad pointer just means
            // we fall back to scanning rather than failing the photo.
            if let Some((off, len)) = raf_header_range(raw) {
                if len > MIN_PREVIEW_BYTES {
                    if let Some(jpeg) = off.checked_add(len).and_then(|end| raw.get(off..end)) {
                        return Ok(jpeg);
                    }
                }
            }
            scan_largest(raw)
        }
        _ => scan_largest(raw),
    }
}

/// The RAF header's JPEG offset/length (big-endian u32s at bytes 84 and 88),
/// if they look sane: the pointed-at bytes must start with a JPEG SOI. The
/// length is not bounds-checked against `raw` so that callers holding only a
/// file prefix can still use the offset.
pub(super) fn raf_header_range(raw: &[u8]) -> Option<(usize, usize)> {
    if !raw.starts_with(RAF_MAGIC) || raw.len() < 92 {
        return None;
    }
    let off = u32::from_be_bytes(raw[84..88].try_into().ok()?) as usize;
    let len = u32::from_be_bytes(raw[88..92].try_into().ok()?) as usize;
    let soi = raw.get(off..off.checked_add(3)?)?;
    (soi == [0xFF, 0xD8, 0xFF] && len > 0).then_some((off, len))
}

fn is_tiff(raw: &[u8]) -> bool {
    match raw.get(..4) {
        Some([b'I', b'I', a, b]) => u16::from_le_bytes([*a, *b]) == 42,
        Some([b'M', b'M', a, b]) => u16::from_be_bytes([*a, *b]) == 42,
        _ => false,
    }
}

/// After FF D8 FF the fourth byte must be a real marker; rejects sensor data
/// that happens to contain FF D8 FF.
fn is_valid_first_marker(byte: u8) -> bool {
    matches!(byte, 0xC0..=0xCF | 0xDB | 0xDD | 0xE0..=0xEF | 0xFE)
}

fn scan_largest(data: &[u8]) -> Result<&[u8], MediaError> {
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i + 4 <= data.len() {
        let starts_jpeg = data[i] == 0xFF
            && data[i + 1] == 0xD8
            && data[i + 2] == 0xFF
            && is_valid_first_marker(data[i + 3]);
        if !starts_jpeg {
            i += 1;
            continue;
        }
        match jpeg_end(data, i) {
            JpegEnd::Found(end) => {
                if end - i > MIN_PREVIEW_BYTES && best.is_none_or(|(s, e)| end - i > e - s) {
                    best = Some((i, end));
                }
                // Resuming after the blob also skips thumbnails nested in its APP segments.
                i = end;
            }
            JpegEnd::Malformed => i += 1,
            // No FF D9 exists past this SOS, so no later candidate (whose scan data
            // starts at or after it) can finish either. Without this stop, a crafted
            // buffer full of SOI+SOS headers rescans the whole tail per candidate.
            JpegEnd::NoEoi => break,
        }
    }
    best.map(|(s, e)| &data[s..e]).ok_or(MediaError::NoJpeg)
}

enum JpegEnd {
    /// One past the EOI.
    Found(usize),
    /// Header walk failed (bad lengths, ran off the buffer): only this candidate is bad.
    Malformed,
    /// Reached the scan data but no FF D9 follows anywhere in the buffer.
    NoEoi,
}

/// The end of the JPEG starting at `start`: walk segments by length
/// until SOS, then look for FF D9 (entropy data stuffs every FF with 00, so
/// the first FF D9 is the true end even across progressive scans).
fn jpeg_end(data: &[u8], start: usize) -> JpegEnd {
    let mut j = start + 2;
    while j + 4 <= data.len() && data[j] == 0xFF {
        let marker = data[j + 1];
        if marker == 0xD9 {
            return JpegEnd::Found(j + 2);
        }
        let seg = u16::from_be_bytes([data[j + 2], data[j + 3]]) as usize;
        if marker == 0xDA {
            let body = j + 2 + seg;
            let found = data
                .get(body..)
                .and_then(|tail| tail.windows(2).position(|w| w == [0xFF, 0xD9]));
            return match found {
                Some(k) => JpegEnd::Found(body + k + 2),
                None => JpegEnd::NoEoi,
            };
        }
        j += 2 + seg;
    }
    JpegEnd::Malformed
}
