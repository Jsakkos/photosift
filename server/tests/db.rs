use std::path::Path;

use chrono::NaiveDate;
use photosift::db::{self, photos, DbPool};
use photosift::outcome::Outcome;
use rusqlite::ErrorCode;

fn open_in(dir: &Path) -> DbPool {
    db::open(&dir.join("photosift.db")).expect("open db")
}

fn is_constraint_violation(err: &anyhow::Error) -> bool {
    matches!(
        err.downcast_ref::<rusqlite::Error>(),
        Some(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation
    )
}

fn seed_shoot(pool: &DbPool) -> i64 {
    let day = NaiveDate::from_ymd_opt(2026, 3, 14).unwrap();
    photos::insert_shoot(
        pool,
        &photos::NewShoot {
            kind: photos::ShootKind::Named,
            name: Some("Park Day".into()),
            folder: r"E:\photos\DSLR\2026\2026-03-14_Park-Day".into(),
            date_start: Some(day),
            date_end: Some(day),
        },
    )
    .expect("insert shoot")
}

fn seed_photo(pool: &DbPool, shoot_id: i64, sha: &str) -> i64 {
    photos::insert_photo(
        pool,
        &photos::NewPhoto {
            shoot_id,
            raw_path: Some(format!(r"E:\x\{sha}.NEF")),
            sha256: sha.into(),
            ..Default::default()
        },
    )
    .expect("insert photo")
}

#[test]
fn migrate_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    drop(open_in(dir.path()));
    let pool = open_in(dir.path());
    let version: i64 = pool
        .with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(version, db::migrations::STEPS.last().unwrap().0);
}

#[test]
fn duplicate_sha_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    photos::insert_file(&pool, "abc", r"E:\a\one.NEF", "one.NEF", 10, None).unwrap();
    let err = photos::insert_file(&pool, "abc", r"E:\a\two.NEF", "two.NEF", 10, None).unwrap_err();
    assert!(is_constraint_violation(&err), "got {err:?}");
}

#[test]
fn duplicate_photo_sha_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    let shoot = seed_shoot(&pool);
    seed_photo(&pool, shoot, "abc");
    let err = photos::insert_photo(
        &pool,
        &photos::NewPhoto {
            shoot_id: shoot,
            sha256: "abc".into(),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(is_constraint_violation(&err), "got {err:?}");
}

#[test]
fn set_outcome_persists() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let pool = open_in(dir.path());
        let shoot = seed_shoot(&pool);
        let id = seed_photo(&pool, shoot, "abc");
        let row = photos::set_outcome(&pool, id, Outcome::Album, Some(true)).unwrap();
        assert_eq!(row.outcome, Outcome::Album);
        assert!(row.blooper);
        id
    };
    let pool = open_in(dir.path());
    let (outcome, blooper): (String, i64) = pool
        .with(|c| {
            c.query_row(
                "SELECT outcome, blooper FROM photos WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(outcome, "album");
    assert_eq!(blooper, 1);
}

#[test]
fn set_outcome_without_blooper_keeps_existing_tag() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    let shoot = seed_shoot(&pool);
    let id = seed_photo(&pool, shoot, "abc");
    photos::set_outcome(&pool, id, Outcome::Library, Some(true)).unwrap();
    let row = photos::set_outcome(&pool, id, Outcome::Archive, None).unwrap();
    assert_eq!(row.outcome, Outcome::Archive);
    assert!(row.blooper);
    let row = photos::set_outcome(&pool, id, Outcome::Archive, Some(false)).unwrap();
    assert!(!row.blooper);
}

#[test]
fn set_outcome_unknown_photo_errors() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    assert!(photos::set_outcome(&pool, 999, Outcome::Album, None).is_err());
}

#[test]
fn outcome_check_constraint() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    let shoot = seed_shoot(&pool);
    seed_photo(&pool, shoot, "abc");
    let err = pool
        .with(|c| c.execute("UPDATE photos SET outcome='pick'", []))
        .unwrap_err();
    assert!(is_constraint_violation(&err), "got {err:?}");
}

#[test]
fn photo_requires_existing_shoot() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    let err = photos::insert_photo(
        &pool,
        &photos::NewPhoto {
            shoot_id: 42,
            sha256: "abc".into(),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(is_constraint_violation(&err), "got {err:?}");
}

#[test]
fn get_photo_round_trips_inserted_fields() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    let shoot = seed_shoot(&pool);
    let taken = NaiveDate::from_ymd_opt(2026, 3, 14)
        .unwrap()
        .and_hms_opt(10, 30, 5)
        .unwrap();
    let id = photos::insert_photo(
        &pool,
        &photos::NewPhoto {
            shoot_id: shoot,
            raw_path: Some("a.NEF".into()),
            jpeg_path: Some("a.JPG".into()),
            sha256: "abc".into(),
            camera: Some("D750".into()),
            capture_time_orig: Some(taken),
            capture_time: Some(taken),
            time_source: Some(photos::TimeSource::Exif),
            orientation: Some(6),
            width: Some(4016),
            height: Some(6016),
            // High bit set: must survive the i64 storage round trip.
            phash: Some(u64::MAX - 7),
            ..Default::default()
        },
    )
    .unwrap();
    let row = photos::get_photo(&pool, id).unwrap().expect("row");
    assert_eq!(row.shoot_id, shoot);
    assert_eq!(row.raw_path.as_deref(), Some("a.NEF"));
    assert_eq!(row.capture_time, Some(taken));
    assert_eq!(row.time_source, Some(photos::TimeSource::Exif));
    assert_eq!((row.width, row.height), (Some(4016), Some(6016)));
    assert_eq!(row.phash, Some(u64::MAX - 7));
    assert_eq!(row.outcome, Outcome::Unreviewed);
    assert!(!row.blooper);
    assert!(photos::get_photo(&pool, id + 1).unwrap().is_none());
}

#[test]
fn known_hashes_returns_only_registered_files() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    photos::insert_file(&pool, "aaa", "a.NEF", "a.NEF", 1, None).unwrap();
    photos::insert_file(&pool, "bbb", "b.JPG", "b.JPG", 2, None).unwrap();
    let known = photos::known_hashes(&pool, &["aaa".into(), "zzz".into(), "bbb".into()]).unwrap();
    assert_eq!(known.len(), 2);
    assert!(known.contains("aaa") && known.contains("bbb"));
    assert!(photos::known_hashes(&pool, &[]).unwrap().is_empty());
}

#[test]
fn known_hashes_handles_more_than_sqlite_variable_limit() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    photos::insert_file(&pool, "h-1500", "p", "p", 1, None).unwrap();
    let probe: Vec<String> = (0..3000).map(|i| format!("h-{i}")).collect();
    let known = photos::known_hashes(&pool, &probe).unwrap();
    assert_eq!(known.len(), 1);
}

#[test]
fn deleting_photo_cascades_to_files_and_faces() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_in(dir.path());
    let shoot = seed_shoot(&pool);
    let id = seed_photo(&pool, shoot, "abc");
    photos::insert_file(&pool, "abc", "a.NEF", "a.NEF", 1, Some(id)).unwrap();
    pool.with(|c| {
        c.execute(
            "INSERT INTO faces(photo_id, x, y, w, h, score) VALUES (?1, 0.1, 0.1, 0.2, 0.2, 0.9)",
            [id],
        )
    })
    .unwrap();
    pool.with(|c| c.execute("DELETE FROM photos WHERE id = ?1", [id]))
        .unwrap();
    let (files, faces): (i64, i64) = pool
        .with(|c| {
            c.query_row(
                "SELECT (SELECT COUNT(*) FROM files), (SELECT COUNT(*) FROM faces)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!((files, faces), (0, 0));
}
