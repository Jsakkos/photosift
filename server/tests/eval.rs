//! The evaluation harness: read-only indexing of a past shoot, matching its
//! `Export\` folder back to originals, and scoring. Synthetic folders only.

mod common;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::NaiveDateTime;
use common::{fake_raf, synthetic_jpeg_plain, synthetic_jpeg_with_exif, ExifSpec, Pattern};
use photosift::eval::{
    self,
    exports::{match_exports, PhotoKey},
    score::score,
    RunOptions,
};
use photosift::outcome::{self, Outcome};
use tempfile::TempDir;

const CAMERA: &str = "FUJIFILM";

fn taken(minute: u32) -> String {
    format!("2026:03:14 10:{minute:02}:00")
}

/// A camera JPEG with DateTimeOriginal `10:<minute>` and Make `FUJIFILM`.
fn original(pattern: Pattern, minute: u32) -> Vec<u8> {
    synthetic_jpeg_with_exif(
        640,
        480,
        pattern,
        &ExifSpec {
            make: Some(CAMERA),
            taken: Some(&taken(minute)),
            ..Default::default()
        },
    )
}

/// What Lightroom would export for the original taken at `minute`: other
/// bytes (smaller), the same EXIF.
fn export_of(pattern: Pattern, minute: u32) -> Vec<u8> {
    synthetic_jpeg_with_exif(
        320,
        240,
        pattern,
        &ExifSpec {
            make: Some(CAMERA),
            taken: Some(&taken(minute)),
            ..Default::default()
        },
    )
}

fn pattern(i: u32) -> Pattern {
    if i.is_multiple_of(2) {
        Pattern::Blocks
    } else {
        Pattern::Stripes
    }
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// `DSCF0001.JPG` .. `DSCF000n.JPG`, two minutes apart (each its own moment).
fn shoot_with_originals(dir: &Path, n: u32) {
    for i in 1..=n {
        write(
            &dir.join(format!("DSCF{i:04}.JPG")),
            &original(pattern(i), i * 2),
        );
    }
}

fn options(eval_dir: &Path, shoots: Vec<PathBuf>) -> RunOptions {
    RunOptions {
        eval_dir: eval_dir.to_path_buf(),
        shoots,
        helper: false,
        max_shoots: None,
        label: "test".into(),
        // Never the real `claude` on PATH.
        overrides: photosift::Overrides {
            claude_bin: Some(eval_dir.join("no-claude-in-tests.exe")),
            ..Default::default()
        },
        helper_timeout: std::time::Duration::from_secs(60),
    }
}

/// Every file and directory under `root`: relative path → (size, mtime) for
/// files, `None` for directories. A directory's own mtime is left out: NTFS
/// can update it lazily after the fixture's last write, and any file created
/// or removed shows up in the listing anyway.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<(u64, SystemTime)>> {
    let mut out = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let meta = entry.metadata().unwrap();
            let rel = entry.path().strip_prefix(root).unwrap().to_path_buf();
            if meta.is_dir() {
                out.insert(rel, None);
                pending.push(entry.path());
            } else {
                out.insert(rel, Some((meta.len(), meta.modified().unwrap())));
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn export_dir_never_indexed() {
    let shoot = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    shoot_with_originals(shoot.path(), 4);
    // Exports carry other bytes than their originals, so content dedupe alone
    // would not keep them out of the index.
    write(
        &shoot.path().join("Export").join("DSCF0001.jpg"),
        &export_of(pattern(1), 2),
    );
    write(
        &shoot.path().join("Export").join("DSCF0003.jpg"),
        &export_of(pattern(3), 6),
    );

    let rows = eval::run(&options(eval_dir.path(), vec![shoot.path().into()]))
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].photos, 4,
        "Export\\ must not be indexed as originals"
    );
    assert_eq!(rows[0].truth, 2);
    assert!(rows[0].unmatched.is_empty(), "{:?}", rows[0].unmatched);

    // The exclusion holds at any depth and ignores case.
    let nested = TempDir::new().unwrap();
    shoot_with_originals(nested.path(), 1);
    write(
        &nested.path().join("day2").join("export").join("x.jpg"),
        &export_of(pattern(1), 2),
    );
    let pool = photosift::db::open(&eval_dir.path().join("t.db")).unwrap();
    let opts = photosift::ingest::scan::ScanOptions {
        exclude_dirs: eval::EXCLUDE_DIRS,
    };
    let scan = photosift::ingest::scan::scan_with_options(
        nested.path(),
        &pool,
        &photosift::config::Settings::default(),
        &opts,
    )
    .unwrap();
    assert_eq!(scan.items.len(), 1);
    // `scan` itself keeps today's behaviour.
    let all = photosift::ingest::scan::scan(
        nested.path(),
        &pool,
        &photosift::config::Settings::default(),
    )
    .unwrap();
    assert_eq!(all.items.len(), 2);
}

fn key(id: i64, stem: &str, minute: u32) -> PhotoKey {
    PhotoKey {
        id,
        stem: stem.into(),
        capture_time: Some(
            NaiveDateTime::parse_from_str(&taken(minute), "%Y:%m:%d %H:%M:%S").unwrap(),
        ),
        camera: Some(CAMERA.into()),
    }
}

#[test]
fn export_matching_by_stem_then_exif() {
    let dir = TempDir::new().unwrap();
    // Rule 1: the stem with a Lightroom suffix; its EXIF points elsewhere.
    write(
        &dir.path().join("DSCF0001-Edit.jpg"),
        &export_of(Pattern::Blocks, 40),
    );
    write(
        &dir.path().join("dscf0002 (2).JPG"),
        &synthetic_jpeg_plain(64, 48),
    );
    // Rule 2: a renamed export, same DateTimeOriginal + camera as DSCF0003.
    write(
        &dir.path().join("family-01.jpg"),
        &export_of(Pattern::Stripes, 6),
    );
    let photos = [
        key(11, "DSCF0001", 2),
        key(12, "DSCF0002", 4),
        key(13, "DSCF0003", 6),
        key(14, "DSCF0004", 8),
    ];

    let m = match_exports(dir.path(), &photos);

    let by_name: HashMap<String, i64> = m
        .matched
        .iter()
        .map(|(p, id)| (p.file_name().unwrap().to_string_lossy().into_owned(), *id))
        .collect();
    assert_eq!(by_name.get("DSCF0001-Edit.jpg"), Some(&11), "{m:?}");
    assert_eq!(by_name.get("dscf0002 (2).JPG"), Some(&12), "{m:?}");
    assert_eq!(by_name.get("family-01.jpg"), Some(&13), "{m:?}");
    assert!(m.unmatched.is_empty(), "{m:?}");

    // Two photos with the same time and camera: rule 2 needs exactly one.
    let twins = [key(21, "A", 6), key(22, "B", 6)];
    let m = match_exports(dir.path(), &twins);
    assert!(m.unmatched.contains(&"family-01.jpg".to_string()), "{m:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn unmatched_exports_reported() {
    // A clip in Export\ can never match (videos are not indexed), but it is
    // still listed rather than dropped.
    let clips = TempDir::new().unwrap();
    write(&clips.path().join("DSCF0009.MOV"), b"not a photo");
    let m = match_exports(clips.path(), &[key(1, "DSCF0002", 4)]);
    assert_eq!(m.unmatched, ["DSCF0009.MOV"]);

    let shoot = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    shoot_with_originals(shoot.path(), 3);
    write(
        &shoot.path().join("Export").join("DSCF0002.jpg"),
        &export_of(pattern(2), 4),
    );
    // No EXIF, unknown stem: nothing to match it by.
    write(
        &shoot.path().join("Export").join("family-02.jpg"),
        &synthetic_jpeg_plain(64, 48),
    );

    let m = match_exports(&shoot.path().join("Export"), &[key(1, "DSCF0002", 4)]);
    assert_eq!(m.unmatched, ["family-02.jpg"]);

    let rows = eval::run(&options(eval_dir.path(), vec![shoot.path().into()]))
        .await
        .unwrap();
    assert_eq!(rows[0].unmatched, ["family-02.jpg"]);
    assert_eq!(rows[0].truth, 1);
    let table = eval::format_table(&rows);
    assert!(table.contains(&rows[0].shoot_hash), "{table}");

    // The row is also appended to results.jsonl.
    let jsonl = std::fs::read_to_string(eval_dir.path().join("results.jsonl")).unwrap();
    let line: serde_json::Value = serde_json::from_str(jsonl.lines().last().unwrap()).unwrap();
    assert_eq!(line["unmatched"], serde_json::json!(["family-02.jpg"]));
    assert_eq!(line["label"], "test");
    assert_eq!(line["shoot_hash"].as_str().unwrap().len(), 8);
    assert!(line["settings_hash"].is_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn index_in_place_writes_nothing_to_library() {
    let shoot = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    shoot_with_originals(shoot.path(), 4);
    write(
        &shoot.path().join("Export").join("DSCF0004.jpg"),
        &export_of(pattern(4), 8),
    );
    let before = snapshot(shoot.path());

    let rows = eval::run(&options(eval_dir.path(), vec![shoot.path().into()]))
        .await
        .unwrap();
    // A second invocation reuses the scratch DB instead of indexing again.
    let again = eval::run(&options(eval_dir.path(), vec![shoot.path().into()]))
        .await
        .unwrap();

    assert_eq!(snapshot(shoot.path()), before, "the library folder changed");
    assert_eq!(rows[0].photos, 4);
    assert_eq!(again[0].shoot_id, rows[0].shoot_id);
    assert_eq!(again[0].photos, 4);
    // Baseline keeps |truth| frames.
    assert_eq!(rows[0].keep, 1);

    let root = eval_dir.path().join("root");
    let previews = std::fs::read_dir(root.join("cache").join("preview"))
        .unwrap()
        .count();
    assert_eq!(previews, 4);
    // Rows point at the originals, and the shoot's folder is the source.
    let pool = photosift::db::open(&root.join("photosift.db")).unwrap();
    let shoot_row = photosift::db::review::get_shoot(&pool, rows[0].shoot_id)
        .unwrap()
        .unwrap();
    assert_eq!(Path::new(&shoot_row.folder), shoot.path());
    let photo = photosift::db::photos::get_photo(&pool, 1).unwrap().unwrap();
    assert!(
        Path::new(photo.jpeg_path.as_deref().unwrap()).starts_with(shoot.path()),
        "{photo:?}"
    );
    let settings = std::fs::read_to_string(root.join("settings.toml")).unwrap();
    assert!(settings.contains("helper_enabled = false"), "{settings}");
}

#[tokio::test(flavor = "multi_thread")]
async fn read_only_refuses_xmp_and_trash() {
    let shoot = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    shoot_with_originals(shoot.path(), 2);
    let root = eval_dir.path().join("root");
    let state = eval::eval_state(
        &root,
        0,
        photosift::Overrides {
            claude_bin: Some(eval_dir.path().join("no-claude-in-tests.exe")),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(state.settings.read_only);
    let shoot_id = {
        let state = state.clone();
        let folder = shoot.path().to_path_buf();
        tokio::task::spawn_blocking(move || eval::index_in_place(&state, &folder))
            .await
            .unwrap()
            .unwrap()
    };
    let before = snapshot(shoot.path());

    let (photo, xmp_error) = outcome::set_and_mirror(&state.db, 1, Outcome::Delete, None, true)
        .expect("the database change stands");
    assert_eq!(photo.outcome, Outcome::Delete);
    assert_eq!(xmp_error.as_deref(), Some("read-only"));
    assert!(!shoot.path().join("DSCF0001.xmp").exists());

    let err = photosift::trash::trash_approved_deletes(&state.db, shoot_id, &[1], true)
        .expect_err("read-only must refuse the Recycle Bin");
    assert!(format!("{err:#}").contains("read-only"), "{err:#}");
    assert!(shoot.path().join("DSCF0001.JPG").exists());

    let err = photosift::xmp::apply(
        &shoot.path().join("DSCF0002.JPG"),
        &outcome::xmp_fields(Outcome::Album, false),
        true,
    )
    .unwrap_err();
    assert!(matches!(err, photosift::xmp::XmpError::ReadOnly), "{err}");
    assert_eq!(snapshot(shoot.path()), before);
}

#[test]
fn score_counts() {
    let truth: HashSet<i64> = [1, 2, 3].into();
    let keep: HashSet<i64> = [1, 4, 9].into();
    let burst_of: HashMap<i64, (i64, i64)> = [
        (1, (0, 0)),
        (2, (1, 1)),
        (4, (1, 1)),
        (3, (2, 2)),
        (9, (3, 3)),
    ]
    .into();

    let s = score(&keep, &truth, &burst_of);

    assert_eq!((s.exact, s.near, s.missed, s.false_picks), (1, 1, 1, 1));
    assert!((s.recall_lenient - 2.0 / 3.0).abs() < 1e-9);
    assert!((s.recall_strict - 1.0 / 3.0).abs() < 1e-9);
    assert!((s.precision_strict - 1.0 / 3.0).abs() < 1e-9);

    // One-to-one: two picks in one burst cover one truth photo there.
    let keep: HashSet<i64> = [4, 5].into();
    let mut burst_of = burst_of;
    burst_of.insert(5, (1, 1));
    let s = score(&keep, &[2].into(), &burst_of);
    assert_eq!((s.exact, s.near, s.missed, s.false_picks), (0, 1, 0, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn raw_subfolder_found() {
    let shoot = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    for i in 1..=3u32 {
        write(
            &shoot.path().join("RAW").join(format!("DSCF{i:04}.RAF")),
            &fake_raf(&original(pattern(i), i * 2)),
        );
    }
    write(
        &shoot.path().join("Export").join("DSCF0002.jpg"),
        &export_of(pattern(2), 4),
    );

    let rows = eval::run(&options(eval_dir.path(), vec![shoot.path().into()]))
        .await
        .unwrap();

    assert_eq!(rows[0].photos, 3);
    assert_eq!(rows[0].truth, 1);
    assert!(rows[0].unmatched.is_empty(), "{:?}", rows[0].unmatched);
}
