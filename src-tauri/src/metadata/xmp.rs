use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Get the XMP sidecar path for an image file.
pub fn sidecar_path(image_path: &Path) -> PathBuf {
    image_path.with_extension("xmp")
}

/// Read the star rating from an existing XMP sidecar.
pub fn read_rating(image_path: &Path) -> Option<i32> {
    let xmp_path = sidecar_path(image_path);
    let content = std::fs::read_to_string(&xmp_path).ok()?;
    parse_rating_from_xml(&content)
}

/// Read the XMP label from an existing sidecar and map to PhotoSift flag.
/// Returns Some("pick") for Green-ish labels, Some("reject") for Red-ish,
/// None otherwise (or if no sidecar).
///
/// Kept for backward-compat re-import of sidecars written by older PhotoSift
/// builds; current `write_cull_metadata` no longer emits `xmp:Label`, so new
/// exports will not round-trip a flag through this path.
pub fn read_flag_from_label(image_path: &Path) -> Option<String> {
    let xmp_path = sidecar_path(image_path);
    let content = std::fs::read_to_string(&xmp_path).ok()?;
    let label = parse_label_from_xml(&content)?;
    let lower = label.to_ascii_lowercase();
    if lower.contains("green") || lower == "pick" {
        Some("pick".into())
    } else if lower.contains("red") || lower == "reject" {
        Some("reject".into())
    } else {
        None
    }
}

fn parse_label_from_xml(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                for attr in e.attributes().flatten() {
                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                    if key == "xmp:Label" {
                        let val = std::str::from_utf8(&attr.value).unwrap_or("");
                        return Some(val.to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn parse_rating_from_xml(xml: &str) -> Option<i32> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                for attr in e.attributes().flatten() {
                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                    if key == "xmp:Rating" {
                        let val = std::str::from_utf8(&attr.value).unwrap_or("");
                        return val.parse().ok();
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

/// Write or update the star rating in an XMP sidecar.
pub fn write_rating(image_path: &Path, rating: i32) -> Result<(), String> {
    let xmp_path = sidecar_path(image_path);

    if xmp_path.exists() {
        let content = std::fs::read_to_string(&xmp_path).map_err(|e| e.to_string())?;
        let updated = update_rating_in_xml(&content, rating)?;
        std::fs::write(&xmp_path, updated).map_err(|e| e.to_string())
    } else {
        let xml = create_xmp_with_rating(rating);
        std::fs::write(&xmp_path, xml).map_err(|e| e.to_string())
    }
}

/// Write culling metadata (rating + PhotoSift destination) to the XMP sidecar
/// alongside `image_path`. If a sidecar already exists, its other contents are
/// preserved; only the two managed attributes are updated (with `xmp:` and
/// `photosift:` namespaces added as needed).
///
/// We deliberately do **not** write `xmp:Label`. The pick/reject flag is carried
/// by the filesystem (rejects live under `RAW/rejects/`, picks under
/// `RAW/edit/` etc.), and emitting Green/Red labels would surface as spurious
/// color labels in Capture One and DxO — which honor `xmp:Label` as a real
/// color-tag signal. Any pre-existing user-set label is passed through
/// unchanged.
pub fn write_cull_metadata(
    image_path: &Path,
    rating: i32,
    destination: &str,
) -> Result<(), String> {
    let xmp_path = sidecar_path(image_path);
    let (base, existing) = if xmp_path.exists() {
        let content = std::fs::read_to_string(&xmp_path).map_err(|e| e.to_string())?;
        (content.clone(), Some(content))
    } else {
        (empty_xmp_skeleton().to_string(), None)
    };

    let updated = update_cull_attrs_in_xml(&base, rating, destination)?;

    // Skip the disk write when the sidecar already matches. Every sync pass
    // runs this for every photo (including the skip branch for files already
    // at their target); avoiding no-op writes keeps sync cheap and preserves
    // sidecar mtime for unchanged photos. Only meaningful when `existing` is
    // Some — a fresh file is always written.
    if let Some(content) = existing {
        if updated == content {
            return Ok(());
        }
    }

    std::fs::write(&xmp_path, updated).map_err(|e| e.to_string())
}

// Minimal XMP skeleton that the parser can inject our managed attributes
// into. Routes every write through `update_cull_attrs_in_xml` so the
// bytes we produce are stable across create/update paths — otherwise a
// pretty-printed create followed by a parser-canonical update would
// rewrite the file on every pass just to normalize formatting.
fn empty_xmp_skeleton() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description></rdf:Description></rdf:RDF></x:xmpmeta>"#
}

fn update_cull_attrs_in_xml(
    xml: &str,
    rating: i32,
    destination: &str,
) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                let elem = if has_rdf_description_name(e) {
                    update_cull_attrs(e, rating, destination)
                } else {
                    e.clone()
                };
                writer.write_event(Event::Start(elem)).map_err(|e| e.to_string())?;
            }
            Ok(Event::Empty(ref e)) => {
                let elem = if has_rdf_description_name(e) {
                    update_cull_attrs(e, rating, destination)
                } else {
                    e.clone()
                };
                writer.write_event(Event::Empty(elem)).map_err(|e| e.to_string())?;
            }
            Ok(event) => {
                writer.write_event(event).map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(format!("XML parse error: {}", e)),
        }
        buf.clear();
    }

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).map_err(|e| e.to_string())
}

fn update_cull_attrs(
    e: &BytesStart,
    rating: i32,
    destination: &str,
) -> BytesStart<'static> {
    let mut new_elem = BytesStart::new(
        String::from_utf8_lossy(e.name().as_ref()).to_string(),
    );

    let mut saw_rating = false;
    let mut saw_dest = false;
    let mut has_xmp_ns = false;
    let mut has_photosift_ns = false;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        match key.as_str() {
            "xmp:Rating" => {
                new_elem.push_attribute(("xmp:Rating", rating.to_string().as_str()));
                saw_rating = true;
            }
            "photosift:destination" => {
                new_elem.push_attribute(("photosift:destination", destination));
                saw_dest = true;
            }
            _ => {
                // Every other attribute passes through untouched — including a
                // user-set `xmp:Label`, which we intentionally do not manage.
                let val = String::from_utf8_lossy(&attr.value).to_string();
                new_elem.push_attribute((key.as_str(), val.as_str()));
                if key == "xmlns:xmp" {
                    has_xmp_ns = true;
                } else if key == "xmlns:photosift" {
                    has_photosift_ns = true;
                }
            }
        }
    }

    if !has_xmp_ns {
        new_elem.push_attribute(("xmlns:xmp", "http://ns.adobe.com/xap/1.0/"));
    }
    if !has_photosift_ns {
        new_elem.push_attribute(("xmlns:photosift", "https://photosift.local/ns/1.0/"));
    }
    if !saw_rating {
        new_elem.push_attribute(("xmp:Rating", rating.to_string().as_str()));
    }
    if !saw_dest {
        new_elem.push_attribute(("photosift:destination", destination));
    }

    new_elem
}

fn create_xmp_with_rating(rating: i32) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmp:Rating="{}">
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#,
        rating
    )
}

fn update_rating_in_xml(xml: &str, rating: i32) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                let elem = if has_rdf_description_name(e) {
                    update_or_add_rating_attr(e, rating)
                } else {
                    e.clone()
                };
                writer.write_event(Event::Start(elem)).map_err(|e| e.to_string())?;
            }
            Ok(Event::Empty(ref e)) => {
                let elem = if has_rdf_description_name(e) {
                    update_or_add_rating_attr(e, rating)
                } else {
                    e.clone()
                };
                writer.write_event(Event::Empty(elem)).map_err(|e| e.to_string())?;
            }
            Ok(event) => {
                writer.write_event(event).map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(format!("XML parse error: {}", e)),
        }
        buf.clear();
    }

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).map_err(|e| e.to_string())
}

fn has_rdf_description_name(e: &BytesStart) -> bool {
    let binding = e.name();
    let name = std::str::from_utf8(binding.as_ref()).unwrap_or("");
    name == "rdf:Description"
}

fn update_or_add_rating_attr(e: &BytesStart, rating: i32) -> BytesStart<'static> {
    let mut new_elem = BytesStart::new(
        String::from_utf8_lossy(e.name().as_ref()).to_string(),
    );

    let mut has_rating = false;
    let mut has_xmp_ns = false;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        if key == "xmp:Rating" {
            new_elem.push_attribute(("xmp:Rating", rating.to_string().as_str()));
            has_rating = true;
        } else {
            let val = String::from_utf8_lossy(&attr.value).to_string();
            new_elem.push_attribute((key.as_str(), val.as_str()));
            if key == "xmlns:xmp" {
                has_xmp_ns = true;
            }
        }
    }

    if !has_rating {
        if !has_xmp_ns {
            new_elem.push_attribute(("xmlns:xmp", "http://ns.adobe.com/xap/1.0/"));
        }
        new_elem.push_attribute(("xmp:Rating", rating.to_string().as_str()));
    }

    new_elem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_xmp_with_rating() {
        let xml = create_xmp_with_rating(3);
        assert!(xml.contains("xmp:Rating=\"3\""));
        assert!(xml.contains("xmlns:xmp"));
    }

    #[test]
    fn test_parse_rating_from_xml() {
        let xml = create_xmp_with_rating(5);
        assert_eq!(parse_rating_from_xml(&xml), Some(5));
    }

    #[test]
    fn test_parse_rating_missing() {
        let xml = r#"<?xml version="1.0"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description/></rdf:RDF></x:xmpmeta>"#;
        assert_eq!(parse_rating_from_xml(xml), None);
    }

    #[test]
    fn test_update_rating_in_existing_xml() {
        let xml = create_xmp_with_rating(2);
        let updated = update_rating_in_xml(&xml, 4).unwrap();
        assert_eq!(parse_rating_from_xml(&updated), Some(4));
    }

    #[test]
    fn test_merge_preserves_other_content() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmp:Rating="2"
      xmp:CreatorTool="DxO PhotoLab">
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#;

        let updated = update_rating_in_xml(xml, 5).unwrap();
        assert!(updated.contains("xmp:Rating=\"5\""));
        assert!(updated.contains("DxO PhotoLab"));
        assert!(updated.contains("xmlns:dc"));
    }

    #[test]
    fn test_sidecar_path() {
        assert_eq!(
            sidecar_path(Path::new("/photos/DSC_1234.NEF")),
            PathBuf::from("/photos/DSC_1234.xmp")
        );
    }

    #[test]
    fn test_write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.nef");
        std::fs::write(&img_path, b"fake").unwrap();

        write_rating(&img_path, 3).unwrap();
        assert_eq!(read_rating(&img_path), Some(3));

        write_rating(&img_path, 5).unwrap();
        assert_eq!(read_rating(&img_path), Some(5));
    }

    #[test]
    fn test_write_cull_metadata_does_not_emit_xmp_label() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("shot.NEF");
        std::fs::write(&img_path, b"fake").unwrap();

        write_cull_metadata(&img_path, 3, "edit").unwrap();

        let content = std::fs::read_to_string(sidecar_path(&img_path)).unwrap();
        assert!(content.contains("xmp:Rating=\"3\""));
        assert!(content.contains("photosift:destination=\"edit\""));
        assert!(
            !content.contains("xmp:Label"),
            "xmp:Label must not be written — it would show as a color label in Capture One / DxO: {}",
            content
        );
    }

    #[test]
    fn test_write_cull_metadata_is_byte_stable_on_no_op() {
        // Writing identical metadata twice must leave the file's mtime
        // untouched. This is what lets sync_shoot_layout safely refresh
        // XMP on every pass without rewriting every sidecar on disk.
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("shot.NEF");
        std::fs::write(&img_path, b"fake").unwrap();

        write_cull_metadata(&img_path, 3, "edit").unwrap();
        let sidecar = sidecar_path(&img_path);
        let first_mtime = std::fs::metadata(&sidecar).unwrap().modified().unwrap();

        // Sleep past the coarsest common filesystem timestamp granularity
        // (FAT: 2s, NTFS: 100ns, ext4: ~1ms) so any real write would shift
        // the mtime even on slow filesystems.
        std::thread::sleep(std::time::Duration::from_millis(20));

        write_cull_metadata(&img_path, 3, "edit").unwrap();
        let second_mtime = std::fs::metadata(&sidecar).unwrap().modified().unwrap();

        assert_eq!(
            first_mtime, second_mtime,
            "no-op write must not touch the sidecar on disk",
        );
    }

    #[test]
    fn test_write_cull_metadata_preserves_existing_user_label() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("shot.NEF");
        std::fs::write(&img_path, b"fake").unwrap();

        // Simulate a sidecar written by Capture One with a user-chosen
        // color label the photographer actually picked. A subsequent
        // PhotoSift write should leave it alone.
        let sidecar = sidecar_path(&img_path);
        std::fs::write(
            &sidecar,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmp:Rating="1"
      xmp:Label="Blue">
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#,
        )
        .unwrap();

        write_cull_metadata(&img_path, 4, "export").unwrap();

        let content = std::fs::read_to_string(&sidecar).unwrap();
        assert!(content.contains("xmp:Rating=\"4\""));
        assert!(
            content.contains("xmp:Label=\"Blue\""),
            "user-set xmp:Label should pass through unchanged: {}",
            content
        );
        assert!(content.contains("photosift:destination=\"export\""));
    }
}
