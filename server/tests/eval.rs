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

#[tokio::test(flavor = "multi_thread")]
async fn helper_mode_scores_the_runs_proposals() {
    let shoot = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    shoot_with_originals(shoot.path(), 4);
    for i in [1, 3] {
        write(
            &shoot.path().join("Export").join(format!("DSCF{i:04}.jpg")),
            &export_of(pattern(i), i * 2),
        );
    }
    let before = snapshot(shoot.path());
    // The fake `claude`: album 1, library 2, archive 3, delete 4, then done.
    let script = eval_dir.path().join("script.json");
    let item = |id: i64, outcome: &str| serde_json::json!({"photo_id": id, "outcome": outcome, "reason": "synthetic"});
    let calls = serde_json::json!([
        {"tool": "propose", "args": {"run_id": "$RUN", "items": [
            item(1, "album"), item(2, "library"), item(3, "archive"), item(4, "delete"),
        ]}},
        {"tool": "finish_run", "args": {"run_id": "$RUN", "summary": "Four frames."}},
    ]);
    std::fs::write(
        &script,
        serde_json::json!({
            "auth": "claude.ai",
            "calls": calls,
            "result": {"is_error": false, "subtype": "success", "result": "All proposed."},
            "exit": 0,
            "delay_ms": 0,
        })
        .to_string(),
    )
    .unwrap();
    let mut opts = options(eval_dir.path(), vec![shoot.path().into()]);
    opts.helper = true;
    opts.overrides = photosift::Overrides {
        claude_bin: Some(PathBuf::from(env!("CARGO_BIN_EXE_fake-claude"))),
        claude_env: vec![(
            "FAKE_CLAUDE_SCRIPT".into(),
            script.to_string_lossy().into_owned(),
        )],
        ..Default::default()
    };

    let rows = eval::run(&opts).await.unwrap();

    let r = &rows[0];
    assert_eq!(r.mode, "helper");
    assert_eq!(r.run_status.as_deref(), Some("done"), "{:?}", r.run_error);
    assert!(r.prompt_version.is_some() && r.model.is_some(), "{r:?}");
    // Keep = album + library = {1, 2}; truth = {1, 3}.
    assert_eq!(r.keep, 2);
    assert_eq!((r.score.exact, r.score.missed), (1, 1));
    let album = r.album.as_ref().expect("album-only score");
    assert_eq!((album.exact, album.false_picks), (1, 0));
    // Usage lands just after finish_run made the run done.
    assert_eq!(r.output_tokens, 300);
    assert!((r.cost_usd - 0.0123).abs() < 1e-9, "{r:?}");
    assert_eq!(snapshot(shoot.path()), before, "the library folder changed");
}

#[tokio::test(flavor = "multi_thread")]
async fn nested_export_dirs_matched() {
    let shoot = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    shoot_with_originals(shoot.path(), 4);
    // Two top-level folders that differ only in case, and one a level down:
    // the scan excludes all three, so all three must supply truth.
    write(
        &shoot.path().join("Export").join("DSCF0001.jpg"),
        &export_of(pattern(1), 2),
    );
    write(
        &shoot.path().join("export").join("DSCF0002.jpg"),
        &export_of(pattern(2), 4),
    );
    write(
        &shoot
            .path()
            .join("day2")
            .join("EXPORT")
            .join("DSCF0004.jpg"),
        &export_of(pattern(4), 8),
    );

    let rows = eval::run(&options(eval_dir.path(), vec![shoot.path().into()]))
        .await
        .unwrap();

    assert_eq!(rows[0].photos, 4);
    assert_eq!(rows[0].truth, 3, "{:?}", rows[0]);
    assert!(rows[0].unmatched.is_empty(), "{:?}", rows[0].unmatched);
}

#[tokio::test(flavor = "multi_thread")]
async fn skipped_shoot_reported() {
    let ok = TempDir::new().unwrap();
    let no_export = TempDir::new().unwrap();
    let eval_dir = TempDir::new().unwrap();
    shoot_with_originals(ok.path(), 2);
    write(
        &ok.path().join("Export").join("DSCF0001.jpg"),
        &export_of(pattern(1), 2),
    );
    shoot_with_originals(no_export.path(), 2);

    let out = eval::run_detailed(&options(
        eval_dir.path(),
        vec![no_export.path().into(), ok.path().into()],
    ))
    .await
    .unwrap();

    assert_eq!(out.rows.len(), 1);
    assert_eq!(out.skipped.len(), 1);
    let hash = eval::shoot_hash(no_export.path());
    assert_eq!(out.skipped[0].shoot_hash, hash);
    let text = eval::format_skipped(&out.skipped);
    assert!(text.contains("skipped 1"), "{text}");
    assert!(text.contains(&hash), "{text}");
    assert!(text.contains("no Export folder"), "{text}");
    assert!(
        !text.contains(&no_export.path().to_string_lossy().to_string()),
        "a path leaked: {text}"
    );
    assert_eq!(eval::format_skipped(&[]), "");
}

fn frame(second: i64, phash: Option<i64>) -> photosift::enrich::moments::MomentInput {
    photosift::enrich::moments::MomentInput {
        capture_time: NaiveDateTime::parse_from_str("2026-03-14T10:00:00", "%Y-%m-%dT%H:%M:%S")
            .unwrap()
            + chrono::Duration::seconds(second),
        phash,
    }
}

/// Four frames one second apart whose pHash distances from the first are
/// 0, 0, 6 and 16 bits; the user kept the first and the last.
fn sweep_shoot() -> eval::sweep::ShootInputs {
    eval::sweep::ShootInputs {
        photos: vec![
            frame(0, Some(0)),
            frame(1, Some(0)),
            frame(2, Some(0b11_1111)),
            frame(3, Some(0xFFFF)),
        ],
        truth: HashSet::from([0, 3]),
    }
}

fn params(phash_max: u32) -> photosift::enrich::moments::SplitParams {
    photosift::enrich::moments::SplitParams {
        moment_gap_seconds: 90,
        burst_window_seconds: 10,
        burst_phash_max: phash_max,
    }
}

#[test]
fn sweep_collision_counts() {
    let shoots = [sweep_shoot()];
    let fine = eval::sweep::evaluate(&shoots, &params(12));
    assert_eq!(fine.truth_collisions, 0.0);
    // Frames 0-2 form one burst, frame 3 its own: 2 bursts of 4 photos.
    assert_eq!(fine.bursts_per_100, 50.0);
    assert_eq!(fine.near_dupe_coverage, 0.5);

    let coarse = eval::sweep::evaluate(&shoots, &params(20));
    assert_eq!(coarse.truth_collisions, 1.0);
    assert_eq!(coarse.bursts_per_100, 25.0);
    assert_eq!(coarse.near_dupe_coverage, 1.0);
}

#[test]
fn sweep_is_pure() {
    let shoots = [sweep_shoot(), sweep_shoot()];
    let a = eval::sweep::evaluate(&shoots, &params(16));
    let b = eval::sweep::evaluate(&shoots, &params(16));
    assert_eq!(a, b);
}

#[test]
fn sweep_recommends_fewest_bursts_within_the_collision_budget() {
    let shoots = [sweep_shoot()];
    let default = params(12);
    let rows = eval::sweep::sweep(&shoots);
    assert_eq!(rows.len(), 8 * 3 * 3);
    let pick = eval::sweep::recommend(&rows, &default).unwrap();
    let base = eval::sweep::evaluate(&shoots, &default);
    assert!(pick.1.truth_collisions <= base.truth_collisions + 0.02);
    assert!(rows
        .iter()
        .filter(|r| r.1.truth_collisions <= base.truth_collisions + 0.02)
        .all(|r| r.1.bursts_per_100 >= pick.1.bursts_per_100));
}
