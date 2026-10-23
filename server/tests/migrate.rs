use photosift::db::{self, SCHEMA_V1};
use rusqlite::Connection;

fn user_version(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn v1_db_upgrades_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photosift.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&format!(
            "BEGIN;\n{SCHEMA_V1}\nPRAGMA user_version = 1;\nCOMMIT;"
        ))
        .unwrap();
        conn.execute(
            "INSERT INTO shoots (kind, folder, created_at) VALUES ('everyday', 'f', '2026-01-01T00:00:00')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photos (shoot_id, sha256, enriched_at) VALUES (1, 'abc', '2026-01-01T00:00:00')",
            [],
        )
        .unwrap();
    }
    assert_eq!(user_version(&path), 1);

    let pool = db::open(&path).unwrap();
    let (sha, faces_model): (String, Option<String>) = pool
        .with(|c| {
            c.query_row("SELECT sha256, faces_model FROM photos", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
        })
        .unwrap();
    assert_eq!(sha, "abc");
    assert_eq!(faces_model, None);
    drop(pool);
    assert_eq!(
        user_version(&path),
        db::migrations::STEPS.last().unwrap().0,
        "user_version is the highest step"
    );
}

/// Plan-1 photos that already have faces were enriched with the same YuNet
/// and SFace files: they are stamped, not re-detected. A photo without faces
/// may simply have none, so it stays NULL and is re-detected once.
#[test]
fn plan1_photos_with_faces_are_stamped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photosift.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&format!(
            "BEGIN;
{SCHEMA_V1}
PRAGMA user_version = 1;
COMMIT;"
        ))
        .unwrap();
        conn.execute_batch(
            "INSERT INTO shoots (kind, folder, created_at) VALUES ('everyday', 'f', '2026-01-01T00:00:00');
             INSERT INTO photos (shoot_id, sha256, enriched_at) VALUES (1, 'a', '2026-01-01T00:00:00');
             INSERT INTO photos (shoot_id, sha256, enriched_at) VALUES (1, 'b', '2026-01-01T00:00:00');
             INSERT INTO photos (shoot_id, sha256, enriched_at) VALUES (1, 'c', '2026-01-01T00:00:00');
             INSERT INTO faces (photo_id, x, y, w, h, embedding) VALUES (1, 0, 0, 1, 1, x'00000000');
             INSERT INTO faces (photo_id, x, y, w, h, embedding) VALUES (1, 0, 0, 1, 1, NULL);
             INSERT INTO faces (photo_id, x, y, w, h, embedding) VALUES (2, 0, 0, 1, 1, NULL);",
        )
        .unwrap();
    }
    let pool = db::open(&path).unwrap();
    let stamps: Vec<Option<String>> = pool
        .with(|c| {
            let mut st = c.prepare("SELECT faces_model FROM photos ORDER BY id")?;
            let rows = st.query_map([], |r| r.get(0))?.collect();
            rows
        })
        .unwrap();
    assert_eq!(
        stamps,
        vec![
            Some(photosift::enrich::FACES_MODEL.to_string()),
            Some(photosift::enrich::FACES_MODEL_DETECTOR_ONLY.to_string()),
            None,
        ]
    );
}

#[test]
fn migrate_is_idempotent_across_versions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photosift.db");
    for _ in 0..3 {
        db::open(&path).unwrap();
    }
    assert_eq!(user_version(&path), db::migrations::STEPS.last().unwrap().0);
}

/// A database left by the removed Tauri prototype: its own tables and a
/// `user_version` that collides with this schema's step numbers.
fn write_prototype_db(path: &std::path::Path, version: i64) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE groups (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE triage_judgments (id INTEGER PRIMARY KEY, group_id INT);
         INSERT INTO groups (name) VALUES ('g');
         PRAGMA user_version = {version};"
    ))
    .unwrap();
}

fn assert_refused_untouched(path: &std::path::Path) {
    let before = std::fs::read(path).unwrap();
    let mtime = std::fs::metadata(path).unwrap().modified().unwrap();

    let err = match db::open(path) {
        Ok(_) => panic!("a prototype database must not open"),
        Err(e) => format!("{e:#}"),
    };
    assert!(err.contains("old PhotoSift prototype"), "{err}");
    assert!(err.contains(&path.display().to_string()), "{err}");
    assert!(err.contains("Nothing was changed"), "{err}");

    assert_eq!(std::fs::read(path).unwrap(), before, "bytes unchanged");
    assert_eq!(
        std::fs::metadata(path).unwrap().modified().unwrap(),
        mtime,
        "mtime unchanged"
    );
    let sidecar = |suffix: &str| {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        std::path::PathBuf::from(p)
    };
    assert!(!sidecar("-wal").exists(), "no WAL file created");
    assert!(!sidecar("-shm").exists(), "no shared-memory file created");
}

#[test]
fn prototype_db_is_refused_and_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photosift.db");
    write_prototype_db(&path, 4);
    assert_refused_untouched(&path);
}

#[test]
fn foreign_db_at_version_zero_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photosift.db");
    write_prototype_db(&path, 0);
    assert_refused_untouched(&path);
}

#[test]
fn every_photosift_version_still_opens() {
    let last = db::migrations::STEPS.last().unwrap().0;
    for version in 1..=last {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photosift.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!(
                "BEGIN;\n{SCHEMA_V1}\nPRAGMA user_version = 1;\nCOMMIT;"
            ))
            .unwrap();
            for &(step, sql) in db::migrations::STEPS.iter().filter(|(n, _)| *n <= version) {
                conn.execute_batch(&format!(
                    "BEGIN;\n{sql}\nPRAGMA user_version = {step};\nCOMMIT;"
                ))
                .unwrap();
            }
        }
        assert_eq!(user_version(&path), version);
        db::open(&path).unwrap_or_else(|e| panic!("v{version}: {e:#}"));
        assert_eq!(
            user_version(&path),
            last,
            "v{version} upgrades to the last step"
        );
    }
}
