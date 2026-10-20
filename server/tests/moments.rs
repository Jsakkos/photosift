mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{NaiveDate, NaiveDateTime};
use futures_util::StreamExt;
use photosift::config::Settings;
use photosift::db::photos::{self, NewPhoto, NewShoot, ShootKind};
use photosift::enrich::moments::{assign, signal_score, split, MomentInput, Signals, SplitParams};
use photosift::media::phash::hamming;
use serde_json::{json, Value};

const PARAMS: SplitParams = SplitParams {
    moment_gap_seconds: 90,
    burst_window_seconds: 10,
    burst_phash_max: 12,
};

fn t(secs: i64) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 18)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap()
        + chrono::Duration::seconds(secs)
}

fn input(secs: i64, phash: Option<i64>) -> MomentInput {
    MomentInput {
        capture_time: t(secs),
        phash,
    }
}

/// A hash `distance` bits away from 0.
fn hash_at(distance: u32) -> i64 {
    ((1u64 << distance) - 1) as i64
}

// ---------------------------------------------------------------- split

#[test]
fn moment_gap_splits_sections() {
    let photos = [input(0, Some(0)), input(30, Some(0)), input(200, Some(0))];
    let moments: Vec<usize> = split(&photos, &PARAMS).iter().map(|p| p.0).collect();
    assert_eq!(moments, [0, 0, 1]);
}

#[test]
fn a_gap_of_exactly_the_threshold_does_not_split() {
    let photos = [input(0, Some(0)), input(90, Some(0)), input(181, Some(0))];
    let moments: Vec<usize> = split(&photos, &PARAMS).iter().map(|p| p.0).collect();
    assert_eq!(moments, [0, 0, 1]);
}

#[test]
fn pose_change_starts_new_burst() {
    let far = [input(0, Some(0)), input(1, Some(hash_at(30)))];
    let bursts: Vec<usize> = split(&far, &PARAMS).iter().map(|p| p.1).collect();
    assert_eq!(bursts, [0, 1]);

    let near = [input(0, Some(0)), input(1, Some(hash_at(5)))];
    let bursts: Vec<usize> = split(&near, &PARAMS).iter().map(|p| p.1).collect();
    assert_eq!(bursts, [0, 0]);
    assert_eq!(hamming(0, hash_at(30)), 30);
}

#[test]
fn burst_compares_against_the_bursts_first_frame_not_the_previous_one() {
    // Each step is 8 bits from the previous frame, so a chain would never
    // split; against the first frame the third is 16 bits away and must.
    let photos = [
        input(0, Some(0)),
        input(1, Some(hash_at(8))),
        input(2, Some(hash_at(16))),
    ];
    let bursts: Vec<usize> = split(&photos, &PARAMS).iter().map(|p| p.1).collect();
    assert_eq!(bursts, [0, 0, 1]);
}

#[test]
fn burst_window_splits_identical_frames() {
    // Same hash, but longer than the window apart (and inside one moment).
    let photos = [input(0, Some(0)), input(10, Some(0)), input(21, Some(0))];
    let p = split(&photos, &PARAMS);
    assert_eq!(p.iter().map(|x| x.0).collect::<Vec<_>>(), [0, 0, 0]);
    assert_eq!(p.iter().map(|x| x.1).collect::<Vec<_>>(), [0, 0, 1]);
}

#[test]
fn burst_indexes_run_across_moments() {
    let photos = [
        input(0, Some(0)),
        input(1, Some(0)),
        input(300, Some(0)),
        input(301, Some(0)),
    ];
    assert_eq!(split(&photos, &PARAMS), [(0, 0), (0, 0), (1, 1), (1, 1)]);
}

#[test]
fn missing_phash_starts_its_own_burst_and_never_panics() {
    let photos = [
        input(0, Some(0)),
        input(1, None),
        input(2, Some(0)),
        input(3, None),
        input(4, None),
    ];
    let bursts: Vec<usize> = split(&photos, &PARAMS).iter().map(|p| p.1).collect();
    assert_eq!(bursts, [0, 1, 2, 3, 4]);
}

#[test]
fn empty_and_single_inputs() {
    assert!(split(&[], &PARAMS).is_empty());
    assert_eq!(split(&[input(0, None)], &PARAMS), [(0, 0)]);
}

// ---------------------------------------------------------------- score

fn base() -> Signals {
    Signals {
        face_count: 1,
        largest_face_area: 0.03,
        face_sharpness: Some(300.0),
        sharpness: 300.0,
        clip_hi: 0.0,
        clip_lo: 0.0,
    }
}

#[test]
fn score_prefers_sharp_faces() {
    let sharp = Signals {
        face_sharpness: Some(400.0),
        ..base()
    };
    let soft = Signals {
        face_sharpness: Some(80.0),
        ..base()
    };
    assert!(signal_score(&sharp) > signal_score(&soft));
}

#[test]
fn score_prefers_faces() {
    let with = Signals {
        sharpness: 200.0,
        ..base()
    };
    let without = Signals {
        face_count: 0,
        largest_face_area: 0.0,
        face_sharpness: None,
        sharpness: 200.0,
        ..base()
    };
    assert!(signal_score(&with) > signal_score(&without));
}

#[test]
fn score_penalizes_clipping() {
    let clipped = Signals {
        clip_hi: 0.15,
        ..base()
    };
    assert!(signal_score(&clipped) < signal_score(&base()));
    let crushed = Signals {
        clip_lo: 0.15,
        ..base()
    };
    assert!(signal_score(&crushed) < signal_score(&base()));
}

#[test]
fn score_prefers_sharper_frames_without_faces() {
    let none = |sharpness| Signals {
        face_count: 0,
        largest_face_area: 0.0,
        face_sharpness: None,
        sharpness,
        ..base()
    };
    assert!(signal_score(&none(500.0)) > signal_score(&none(50.0)));
}

#[test]
fn score_is_finite_for_degenerate_signals() {
    let s = Signals {
        face_count: 0,
        largest_face_area: 0.0,
        face_sharpness: None,
        sharpness: 0.0,
        clip_hi: 1.0,
        clip_lo: 1.0,
    };
    assert!(signal_score(&s).is_finite());
}

// ---------------------------------------------------------------- assign

fn photo(shoot_id: i64, n: usize, secs: i64, phash: Option<u64>) -> NewPhoto {
    NewPhoto {
        shoot_id,
        jpeg_path: Some(format!("x/{n}.jpg")),
        sha256: format!("sha-{shoot_id}-{n}"),
        capture_time: Some(t(secs)),
        phash,
        ..Default::default()
    }
}

fn shoot(pool: &photosift::db::DbPool, folder: &str) -> i64 {
    photos::insert_shoot(
        pool,
        &NewShoot {
            kind: ShootKind::Everyday,
            name: None,
            folder: folder.into(),
            date_start: None,
            date_end: None,
        },
    )
    .unwrap()
}

fn cols(pool: &photosift::db::DbPool, id: i64) -> (Option<i64>, Option<i64>, Option<f64>) {
    let p = photos::get_photo(pool, id).unwrap().unwrap();
    (p.moment_idx, p.burst_idx, p.signal_score)
}

#[test]
fn assign_writes_columns_ordered_by_time_then_id() {
    let dir = tempfile::tempdir().unwrap();
    let pool = photosift::db::open(&dir.path().join("p.db")).unwrap();
    let s = shoot(&pool, "a");
    // Inserted out of time order; ids[2] and ids[3] tie on time.
    let rows = [
        (300, Some(0)),
        (0, Some(0)),
        (1, Some(0)),
        (1, Some(u64::MAX >> 20)), // 44 bits away from 0
    ];
    let ids: Vec<i64> = rows
        .iter()
        .enumerate()
        .map(|(n, &(secs, h))| photos::insert_photo(&pool, &photo(s, n, secs, h)).unwrap())
        .collect();
    assign(&pool, s, &Settings::default()).unwrap();

    // Time order: ids[1] (0 s), ids[2] (1 s), ids[3] (1 s, tie -> id order), ids[0] (300 s).
    let got: Vec<_> = ids.iter().map(|&i| cols(&pool, i)).collect();
    assert_eq!((got[1].0, got[1].1), (Some(0), Some(0)));
    assert_eq!((got[2].0, got[2].1), (Some(0), Some(0)));
    assert_eq!((got[3].0, got[3].1), (Some(0), Some(1)));
    assert_eq!((got[0].0, got[0].1), (Some(1), Some(2)));
    assert!(got.iter().all(|g| g.2.is_some()), "scores written");
}

#[test]
fn assign_skips_trashed_and_video_only_rows_and_clears_stale_values() {
    let dir = tempfile::tempdir().unwrap();
    let pool = photosift::db::open(&dir.path().join("p.db")).unwrap();
    let s = shoot(&pool, "a");
    let keep = photos::insert_photo(&pool, &photo(s, 0, 0, Some(0))).unwrap();
    let trashed = photos::insert_photo(&pool, &photo(s, 1, 1, Some(0))).unwrap();
    let after = photos::insert_photo(&pool, &photo(s, 2, 400, Some(0))).unwrap();
    let video = photos::insert_photo(
        &pool,
        &NewPhoto {
            shoot_id: s,
            video_path: Some("x/v.mov".into()),
            sha256: "sha-video".into(),
            capture_time: Some(t(2)),
            ..Default::default()
        },
    )
    .unwrap();
    // A previous run assigned values to the now-trashed row.
    pool.with(|c| {
        c.execute(
            "UPDATE photos SET moment_idx = 5, burst_idx = 5, signal_score = 1.0, \
             trashed_at = '2026-10-18T12:00:00' WHERE id = ?1",
            [trashed],
        )
    })
    .unwrap();
    assign(&pool, s, &Settings::default()).unwrap();

    assert_eq!(cols(&pool, trashed), (None, None, None));
    assert_eq!(cols(&pool, video), (None, None, None));
    // Indexes stay contiguous over the remaining photos.
    assert_eq!(cols(&pool, keep).0, Some(0));
    assert_eq!(cols(&pool, after).0, Some(1));
    assert_eq!(cols(&pool, after).1, Some(1));
}

#[test]
fn assign_is_scoped_to_its_shoot_and_tolerates_missing_phash() {
    let dir = tempfile::tempdir().unwrap();
    let pool = photosift::db::open(&dir.path().join("p.db")).unwrap();
    let (a, b) = (shoot(&pool, "a"), shoot(&pool, "b"));
    let in_a = photos::insert_photo(&pool, &photo(a, 0, 0, None)).unwrap();
    let in_b = photos::insert_photo(&pool, &photo(b, 1, 0, Some(0))).unwrap();
    assign(&pool, a, &Settings::default()).unwrap();
    assert_eq!(cols(&pool, in_a).0, Some(0));
    assert_eq!(
        cols(&pool, in_b),
        (None, None, None),
        "other shoot untouched"
    );
    // An empty or unknown shoot is a no-op.
    assign(&pool, 999, &Settings::default()).unwrap();
}

#[test]
fn assign_scores_from_faces_and_quality() {
    let dir = tempfile::tempdir().unwrap();
    let pool = photosift::db::open(&dir.path().join("p.db")).unwrap();
    let s = shoot(&pool, "a");
    let with = photos::insert_photo(&pool, &photo(s, 0, 0, Some(0))).unwrap();
    let without = photos::insert_photo(&pool, &photo(s, 1, 100, Some(0))).unwrap();
    pool.with(|c| {
        c.execute(
            "UPDATE photos SET sharpness = 200.0, face_sharpness = 300.0, clip_hi = 0.0, \
             clip_lo = 0.0 WHERE id = ?1",
            [with],
        )?;
        c.execute(
            "UPDATE photos SET sharpness = 200.0, clip_hi = 0.0, clip_lo = 0.0 WHERE id = ?1",
            [without],
        )?;
        c.execute(
            "INSERT INTO faces (photo_id, x, y, w, h) VALUES (?1, 0.1, 0.1, 0.2, 0.2)",
            [with],
        )
    })
    .unwrap();
    assign(&pool, s, &Settings::default()).unwrap();
    let (a, b) = (
        cols(&pool, with).2.unwrap(),
        cols(&pool, without).2.unwrap(),
    );
    assert!(a > b, "face photo {a} should outscore {b}");
}

// ---------------------------------------------------------------- golden

fn golden_dir() -> PathBuf {
    // Panic rather than skip: an --ignored run that checked nothing must not look green.
    std::env::var_os("PHOTOSIFT_GOLDEN_DIR")
        .map(PathBuf::from)
        .expect("set PHOTOSIFT_GOLDEN_DIR; see tests/golden/README.md")
}

async fn import_and_enrich(base: &str, source: &Path) {
    let http = reqwest::Client::new();
    let url = |p: &str| format!("{base}{p}");
    let scan: Value = http
        .post(url("/api/scan"))
        .json(&json!({ "source": source }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let events: Vec<Value> = scan["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            json!({
                "item_idxs": e["item_idxs"],
                "kind": e["kind"],
                "name": if e["kind"] == "named" { json!("Moments Test") } else { json!("") },
            })
        })
        .collect();
    let ws_url = url("/ws").replacen("http://", "ws://", 1);
    let (mut ws, _) = tokio_tungstenite::connect_async(ws_url).await.unwrap();
    let resp = http
        .post(url("/api/import"))
        .json(&json!({ "scan_id": scan["scan_id"], "events": events, "camera_offsets": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let job_id = resp.json::<Value>().await.unwrap()["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut shoot_ids: Option<Vec<i64>> = None;
    let mut enriched: HashSet<i64> = HashSet::new();
    tokio::time::timeout(Duration::from_secs(600), async {
        while let Some(msg) = ws.next().await {
            let Ok(text) = msg.unwrap().into_text() else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            match v["type"].as_str() {
                Some("import_done") if v["job_id"] == job_id.as_str() => {
                    shoot_ids = Some(
                        v["shoot_ids"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|s| s.as_i64().unwrap())
                            .collect(),
                    );
                }
                Some("import_failed") => panic!("import failed: {v}"),
                Some("enrich_done") => {
                    enriched.insert(v["shoot_id"].as_i64().unwrap());
                }
                _ => {}
            }
            if let Some(ids) = &shoot_ids {
                if ids.iter().all(|id| enriched.contains(id)) {
                    return;
                }
            }
        }
        panic!("websocket closed early");
    })
    .await
    .expect("import + enrichment did not finish in time");
}

#[tokio::test]
#[ignore = "needs PHOTOSIFT_GOLDEN_DIR (real fixture; run scripts/make-golden-fixture.ps1)"]
async fn golden_burst_groups_near_identicals() {
    let dir = golden_dir();
    let library = tempfile::tempdir().unwrap();
    let server = common::spawn_server_with_library(library.path()).await;
    import_and_enrich(&server.base_url, &dir).await;

    let pool = photosift::db::open(&server.data_root.path().join("photosift.db")).unwrap();
    let by_stem = |stem: &str| -> photos::PhotoRow {
        let id: i64 = pool
            .with(|c| {
                c.query_row(
                    "SELECT id FROM photos WHERE raw_path LIKE ?1 OR jpeg_path LIKE ?1 \
                     ORDER BY id LIMIT 1",
                    [format!("%{stem}.%")],
                    |r| r.get(0),
                )
            })
            .unwrap_or_else(|e| panic!("{stem} not in the golden card: {e}"));
        photos::get_photo(&pool, id).unwrap().unwrap()
    };

    let (moments, bursts, total): (i64, i64, i64) = pool
        .with(|c| {
            c.query_row(
                "SELECT COUNT(DISTINCT shoot_id || ':' || moment_idx), \
                        COUNT(DISTINCT shoot_id || ':' || burst_idx), COUNT(*) \
                 FROM photos WHERE moment_idx IS NOT NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    println!("golden card: {total} photos -> {moments} moments, {bursts} bursts");

    let report = |label: &str, stems: &[&str]| {
        let rows: Vec<_> = stems.iter().map(|s| (s.to_string(), by_stem(s))).collect();
        for (stem, r) in &rows {
            println!(
                "{label} {stem}: time {:?} moment {:?} burst {:?}",
                r.capture_time, r.moment_idx, r.burst_idx
            );
        }
        for w in rows.windows(2) {
            let (a, b) = (&w[0].1, &w[1].1);
            println!(
                "  {} -> {}: gap {:?}, pHash distance {:?}",
                w[0].0,
                w[1].0,
                b.capture_time.zip(a.capture_time).map(|(x, y)| x - y),
                a.phash
                    .zip(b.phash)
                    .map(|(x, y)| hamming(x as i64, y as i64)),
            );
        }
        if let (Some(a), Some(b)) = (rows.first(), rows.last()) {
            println!(
                "  {} -> {} (first to last): pHash distance {:?}",
                a.0,
                b.0,
                a.1.phash
                    .zip(b.1.phash)
                    .map(|(x, y)| hamming(x as i64, y as i64)),
            );
        }
        rows
    };
    let first = report("A", &["DSCF1615", "DSCF1616", "DSCF1617"]);
    let second = report("B", &["DSCF1765", "DSCF1766"]);
    for (label, rows) in [("1615-1617", &first), ("1765-1766", &second)] {
        let b = rows[0].1.burst_idx;
        assert!(b.is_some(), "{label}: no burst assigned");
        assert!(
            rows.iter().all(|(_, r)| r.burst_idx == b),
            "{label} must share one burst"
        );
    }
}
