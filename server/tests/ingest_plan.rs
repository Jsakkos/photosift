mod common;

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, Timelike};
use common::{fake_raf, synthetic_jpeg_plain, synthetic_jpeg_with_exif, ExifSpec, Pattern};
use photosift::config::Settings;
use photosift::db::photos::TimeSource;
use photosift::db::{self, photos, DbPool};
use photosift::ingest::events::{split_into_events, EventKind};
use photosift::ingest::naming::{folder_path, slugify};
use photosift::ingest::scan::{scan, ItemKind, ScanItem};

fn at(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}

fn item(idx: usize, camera: &str, when: &str) -> ScanItem {
    ScanItem {
        idx,
        primary: PathBuf::from(format!("X{idx}.JPG")),
        sibling_jpeg: None,
        kind: ItemKind::Photo,
        camera: camera.into(),
        capture_time: at(when),
        time_source: TimeSource::Exif,
        bytes: 1,
    }
}

fn items_at(camera: &str, times: &[&str]) -> Vec<ScanItem> {
    times
        .iter()
        .enumerate()
        .map(|(i, t)| item(i, camera, t))
        .collect()
}

fn no_offsets() -> HashMap<String, i64> {
    HashMap::new()
}

fn pool() -> (tempfile::TempDir, DbPool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = db::open(&dir.path().join("photosift.db")).unwrap();
    (dir, pool)
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn jpeg_taken(taken: &'static str) -> Vec<u8> {
    synthetic_jpeg_with_exif(
        64,
        48,
        Pattern::Blocks,
        &ExifSpec {
            make: Some("FUJIFILM"),
            model: Some("X100VI"),
            taken: Some(taken),
            ..Default::default()
        },
    )
}

fn local_naive(t: SystemTime) -> NaiveDateTime {
    DateTime::<Local>::from(t)
        .naive_local()
        .with_nanosecond(0)
        .unwrap()
}

// --- naming ---------------------------------------------------------------

#[test]
fn slugify_cases() {
    assert_eq!(slugify("Baby's First Meal"), "Babys-First-Meal");
    assert_eq!(
        slugify("  Birthday — at the Park! "),
        "Birthday-at-the-Park"
    );
    assert_eq!(slugify("Café día"), "Cafe-dia");
    assert_eq!(slugify(""), "Untitled");
    assert_eq!(slugify("  !!! "), "Untitled");
    assert_eq!(slugify("Straße Fest"), "Strasse-Fest");
}

#[test]
fn folder_paths() {
    let root = Path::new(r"E:\photos\DSLR");
    let named = folder_path(
        root,
        EventKind::Named,
        NaiveDate::from_ymd_opt(2026, 10, 18).unwrap(),
        "Birthday Party",
    );
    assert_eq!(
        named,
        PathBuf::from(r"E:\photos\DSLR\2026\2026-10-18_Birthday-Party")
    );
    let everyday = folder_path(
        root,
        EventKind::Everyday,
        NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(),
        "ignored",
    );
    assert_eq!(
        everyday,
        PathBuf::from(r"E:\photos\DSLR\2026\2026-10_Everyday")
    );
}

// --- events ---------------------------------------------------------------

#[test]
fn splits_by_gap_and_date() {
    let items = items_at(
        "A",
        &[
            "2026-10-18 10:00",
            "2026-10-18 10:05",
            "2026-10-18 13:00",
            "2026-10-19 09:00",
        ],
    );
    let events = split_into_events(&items, &no_offsets(), 120, 25);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].item_idxs, vec![0, 1]);
    assert_eq!(events[1].item_idxs, vec![2]);
    assert_eq!(events[2].item_idxs, vec![3]);
    assert_eq!(events[0].start, at("2026-10-18 10:00"));
    assert_eq!(events[0].end, at("2026-10-18 10:05"));
    assert!(events.iter().all(|e| e.suggested_name.is_empty()));
}

#[test]
fn date_change_splits_even_inside_the_gap() {
    let items = items_at("A", &["2026-10-18 23:50", "2026-10-19 00:10"]);
    assert_eq!(split_into_events(&items, &no_offsets(), 120, 25).len(), 2);
}

#[test]
fn gap_exactly_at_threshold_stays_together() {
    let items = items_at("A", &["2026-10-18 10:00", "2026-10-18 12:00"]);
    assert_eq!(split_into_events(&items, &no_offsets(), 120, 25).len(), 1);
}

#[test]
fn small_event_defaults_everyday() {
    let times: Vec<String> = (0..12).map(|m| format!("2026-10-18 10:{m:02}")).collect();
    let refs: Vec<&str> = times.iter().map(String::as_str).collect();
    let small = split_into_events(&items_at("A", &refs), &no_offsets(), 120, 25);
    assert_eq!(small.len(), 1);
    assert_eq!(small[0].kind, EventKind::Everyday);

    let times: Vec<String> = (0..30).map(|m| format!("2026-10-18 10:{m:02}")).collect();
    let refs: Vec<&str> = times.iter().map(String::as_str).collect();
    let big = split_into_events(&items_at("A", &refs), &no_offsets(), 120, 25);
    assert_eq!(big.len(), 1);
    assert_eq!(big[0].kind, EventKind::Named);
}

#[test]
fn videos_do_not_count_toward_event_size() {
    let mut items: Vec<ScanItem> = (0..10)
        .map(|m| item(m, "A", &format!("2026-10-18 10:{m:02}")))
        .collect();
    for (n, it) in items.iter_mut().enumerate() {
        if n >= 4 {
            it.kind = ItemKind::Video;
        }
    }
    // 4 photos + 6 videos with everyday_max = 5: only photos count.
    let events = split_into_events(&items, &no_offsets(), 120, 5);
    assert_eq!(events[0].item_idxs.len(), 10);
    assert_eq!(events[0].kind, EventKind::Everyday);
}

#[test]
fn camera_offset_shifts_items() {
    let items = vec![
        item(0, "A", "2026-10-18 13:00"),
        item(1, "B", "2026-10-18 12:00"),
    ];
    // Without the offset B is an hour earlier but still inside the gap, so use
    // a gap small enough that only the correction makes them one event.
    let apart = split_into_events(&items, &no_offsets(), 30, 25);
    assert_eq!(apart.len(), 2);

    let offsets = HashMap::from([("B".to_string(), 3600i64)]);
    let joined = split_into_events(&items, &offsets, 30, 25);
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].item_idxs.len(), 2);
    assert_eq!(joined[0].start, at("2026-10-18 13:00"));
}

#[test]
fn unsorted_input_is_ordered_chronologically() {
    let items = vec![
        item(0, "A", "2026-10-18 10:30"),
        item(1, "A", "2026-10-18 10:00"),
    ];
    let events = split_into_events(&items, &no_offsets(), 120, 25);
    assert_eq!(events[0].item_idxs, vec![1, 0]);
}

#[test]
fn no_items_no_events() {
    assert!(split_into_events(&[], &no_offsets(), 120, 25).is_empty());
}

// --- scan -----------------------------------------------------------------

#[test]
fn pairs_raf_with_jpg() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = jpeg_taken("2026:10:18 10:00:00");
    write(&dir.path().join("DSCF0001.RAF"), &fake_raf(&jpeg));
    write(&dir.path().join("DSCF0001.JPG"), &jpeg);
    write(
        &dir.path().join("DSCF0002.JPG"),
        &jpeg_taken("2026:10:18 10:01:00"),
    );
    write(&dir.path().join("CLIP0001.MOV"), b"not really a movie");
    let (_d, pool) = pool();

    let card = scan(dir.path(), &pool, &Settings::default()).unwrap();
    assert_eq!(card.items.len(), 3);

    let pair = card
        .items
        .iter()
        .find(|i| i.primary.file_name().unwrap() == "DSCF0001.RAF")
        .expect("RAF is the primary of its pair");
    assert_eq!(pair.kind, ItemKind::Photo);
    assert_eq!(
        pair.sibling_jpeg.as_ref().unwrap().file_name().unwrap(),
        "DSCF0001.JPG"
    );
    assert_eq!(pair.time_source, TimeSource::Exif);
    assert_eq!(pair.capture_time, at("2026-10-18 10:00"));
    assert_eq!(pair.camera, "FUJIFILM X100VI");

    let lone = card
        .items
        .iter()
        .find(|i| i.primary.file_name().unwrap() == "DSCF0002.JPG")
        .unwrap();
    assert!(lone.sibling_jpeg.is_none());

    let video = card
        .items
        .iter()
        .find(|i| i.kind == ItemKind::Video)
        .unwrap();
    assert_eq!(video.primary.file_name().unwrap(), "CLIP0001.MOV");
    assert_eq!(video.time_source, TimeSource::Mtime);

    // idx is the position in items, which events refer back to.
    for (n, i) in card.items.iter().enumerate() {
        assert_eq!(i.idx, n);
    }
    // The pair's bytes cover both files that will be copied.
    assert_eq!(pair.bytes as usize, fake_raf(&jpeg).len() + jpeg.len());
}

#[test]
fn walks_nested_dcim_and_ignores_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = jpeg_taken("2026:10:18 10:00:00");
    write(&dir.path().join("DCIM/100_FUJI/dscf0001.jpg"), &jpeg);
    write(&dir.path().join("DCIM/101_FUJI/DSCF0002.JPEG"), &jpeg);
    write(&dir.path().join("DCIM/100_FUJI/notes.txt"), b"hi");
    write(&dir.path().join("DCIM/100_FUJI/.hidden.jpg"), &jpeg);
    write(
        &dir.path().join("DCIM/100_FUJI/._DSCF0001.JPG"),
        b"mac junk",
    );
    let (_d, pool) = pool();

    let card = scan(dir.path(), &pool, &Settings::default()).unwrap();
    assert_eq!(card.items.len(), 2);
    assert_eq!(card.source, dir.path());
}

#[test]
fn missing_capture_time_uses_mtime() {
    let dir = tempfile::tempdir().unwrap();
    let noexif = dir.path().join("NOEXIF.JPG");
    write(&noexif, &synthetic_jpeg_plain(64, 48));
    let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1_760_000_000);
    File::options()
        .write(true)
        .open(&noexif)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let (_d, pool) = pool();

    let card = scan(dir.path(), &pool, &Settings::default()).unwrap();
    let it = &card.items[0];
    assert_eq!(it.time_source, TimeSource::Mtime);
    assert_eq!(it.capture_time, local_naive(mtime));
    assert_eq!(it.camera, "Unknown");
    assert_eq!(card.events.len(), 1);
    assert_eq!(card.events[0].item_idxs, vec![0]);
}

#[test]
fn exif_beyond_the_prefix_is_found_by_full_read() {
    // The RAF's embedded JPEG starts past the 512 KiB prefix the scan reads
    // first, so only the whole-file retry can see its EXIF.
    let jpeg = jpeg_taken("2026:10:18 11:30:00");
    let offset = 600_000usize;
    let mut raf = vec![0u8; offset];
    raf[..16].copy_from_slice(b"FUJIFILMCCD-RAW ");
    raf[84..88].copy_from_slice(&(offset as u32).to_be_bytes());
    raf[88..92].copy_from_slice(&(jpeg.len() as u32).to_be_bytes());
    raf.extend_from_slice(&jpeg);

    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("DSCF0009.RAF"), &raf);
    let (_d, pool) = pool();

    let card = scan(dir.path(), &pool, &Settings::default()).unwrap();
    assert_eq!(card.items[0].time_source, TimeSource::Exif);
    assert_eq!(card.items[0].capture_time, at("2026-10-18 11:30"));
}

#[test]
fn cameras_events_and_scan_id() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("A1.JPG"),
        &jpeg_taken("2026:10:18 10:00:00"),
    );
    write(
        &dir.path().join("A2.JPG"),
        &jpeg_taken("2026:10:18 10:20:00"),
    );
    write(
        &dir.path().join("A3.JPG"),
        &jpeg_taken("2026:10:19 09:00:00"),
    );
    let (_d, pool) = pool();
    let settings = Settings::default();

    let first = scan(dir.path(), &pool, &settings).unwrap();
    let second = scan(dir.path(), &pool, &settings).unwrap();
    assert!(!first.scan_id.is_empty());
    assert_ne!(first.scan_id, second.scan_id);

    assert_eq!(first.cameras.len(), 1);
    let cam = &first.cameras[0];
    assert_eq!(cam.camera, "FUJIFILM X100VI");
    assert_eq!(cam.count, 3);
    assert_eq!(cam.first, at("2026-10-18 10:00"));
    assert_eq!(cam.last, at("2026-10-19 09:00"));

    assert_eq!(first.events.len(), 2);
    assert_eq!(first.events[0].item_idxs.len(), 2);
    // 2 photos is below everyday_max, so both days default to Everyday.
    assert!(first.events.iter().all(|e| e.kind == EventKind::Everyday));
}

#[test]
fn settings_drive_the_event_split() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("A1.JPG"),
        &jpeg_taken("2026:10:18 10:00:00"),
    );
    write(
        &dir.path().join("A2.JPG"),
        &jpeg_taken("2026:10:18 10:20:00"),
    );
    let (_d, pool) = pool();
    let tight = Settings {
        event_gap_minutes: 10,
        everyday_max_photos: 1,
        ..Settings::default()
    };

    let card = scan(dir.path(), &pool, &tight).unwrap();
    assert_eq!(card.events.len(), 2);
    assert!(card.events.iter().all(|e| e.kind == EventKind::Named));
}

#[test]
fn already_imported_matches_on_name_and_size() {
    let dir = tempfile::tempdir().unwrap();
    let known = jpeg_taken("2026:10:18 10:00:00");
    write(&dir.path().join("DSCF0001.JPG"), &known);
    write(
        &dir.path().join("DSCF0002.JPG"),
        &jpeg_taken("2026:10:18 10:01:00"),
    );
    // Same name as a known file but a different size is a different file.
    write(
        &dir.path().join("DSCF0003.JPG"),
        &jpeg_taken("2026:10:18 10:02:00"),
    );
    let (_d, pool) = pool();

    let shoot = photos::insert_shoot(
        &pool,
        &photos::NewShoot {
            kind: photos::ShootKind::Everyday,
            name: None,
            folder: "E:/x".into(),
            date_start: None,
            date_end: None,
        },
    )
    .unwrap();
    let photo = photos::insert_photo(
        &pool,
        &photos::NewPhoto {
            shoot_id: shoot,
            jpeg_path: Some("E:/x/DSCF0001.JPG".into()),
            sha256: "aa".into(),
            ..Default::default()
        },
    )
    .unwrap();
    photos::insert_file(
        &pool,
        "aa",
        "E:/x/DSCF0001.JPG",
        "DSCF0001.JPG",
        known.len() as i64,
        Some(photo),
    )
    .unwrap();
    photos::insert_file(&pool, "bb", "E:/x/DSCF0003.JPG", "DSCF0003.JPG", 1, None).unwrap();

    let card = scan(dir.path(), &pool, &Settings::default()).unwrap();
    assert_eq!(card.items.len(), 3);
    assert_eq!(card.already_imported, 1);
}
