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
