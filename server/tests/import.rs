mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use common::{fake_raf, spawn_server_with_library, synthetic_jpeg_with_exif, ExifSpec, Pattern};
use futures_util::StreamExt;
use photosift::db::photos::{self, PhotoRow};
use rusqlite::Connection;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Env {
    server: common::TestServer,
    library: TempDir,
    http: reqwest::Client,
}

impl Env {
    async fn new() -> Env {
        let library = tempfile::tempdir().unwrap();
        let server = spawn_server_with_library(library.path()).await;
        Env {
            server,
            library,
            http: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.server.base_url)
    }

    fn db(&self) -> Connection {
        Connection::open(self.server.data_root.path().join("photosift.db")).unwrap()
    }

    /// All photo rows, oldest capture first.
    fn photos(&self) -> Vec<PhotoRow> {
        let pool = photosift::db::open(&self.server.data_root.path().join("photosift.db")).unwrap();
        let ids: Vec<i64> = {
            let db = self.db();
            let mut stmt = db
                .prepare("SELECT id FROM photos ORDER BY capture_time, id")
                .unwrap();
            let ids = stmt
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            ids
        };
        ids.into_iter()
            .map(|id| photos::get_photo(&pool, id).unwrap().unwrap())
            .collect()
    }

    fn cache(&self, kind: &str, id: i64) -> PathBuf {
        self.server
            .data_root
            .path()
            .join("cache")
            .join(kind)
            .join(format!("{id}.jpg"))
    }

    async fn scan(&self, source: &Path) -> Value {
        let resp = self
            .http
            .post(self.url("/api/scan"))
            .json(&json!({ "source": source }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        resp.json().await.unwrap()
    }

    async fn ws(&self) -> Ws {
        let url = self.url("/ws").replacen("http://", "ws://", 1);
        let (ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        ws
    }

    /// Scans `source`, imports every item as one event, returns the WS frames
    /// up to and including the terminal frame for the job.
    async fn import_all(&self, source: &Path, kind: &str, name: &str) -> Vec<Value> {
        self.import_all_with(source, kind, name, json!({})).await
    }

    async fn import_all_with(
        &self,
        source: &Path,
        kind: &str,
        name: &str,
        offsets: Value,
    ) -> Vec<Value> {
        let scan = self.scan(source).await;
        let idxs: Vec<usize> = (0..scan["items"].as_array().unwrap().len()).collect();
        let events = json!([{ "item_idxs": idxs, "kind": kind, "name": name }]);
        self.import(&scan["scan_id"], events, offsets).await
    }

    async fn import(&self, scan_id: &Value, events: Value, offsets: Value) -> Vec<Value> {
        let mut ws = self.ws().await;
        let resp = self
            .http
            .post(self.url("/api/import"))
            .json(&json!({
                "scan_id": scan_id,
                "events": events,
                "camera_offsets": offsets,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let job_id = body["job_id"].as_str().expect("job_id").to_string();
        collect_until_terminal(&mut ws, &job_id).await
    }
}

async fn collect_until_terminal(ws: &mut Ws, job_id: &str) -> Vec<Value> {
    let mut frames = Vec::new();
    let deadline = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(msg) = ws.next().await {
            let msg = msg.unwrap();
            let Ok(text) = msg.into_text() else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if v["job_id"] != job_id {
                continue;
            }
            let terminal = matches!(v["type"].as_str(), Some("import_done" | "import_failed"));
            frames.push(v);
            if terminal {
                return;
            }
        }
        panic!("websocket closed before the job finished");
    });
    deadline.await.expect("import did not finish in time");
    frames
}

fn terminal(frames: &[Value]) -> &Value {
    frames.last().expect("at least a terminal frame")
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

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A 3-frame card on one day: DSCF0001..3, one minute apart.
fn three_file_card() -> TempDir {
    let card = tempfile::tempdir().unwrap();
    for (n, t) in [
        "2026:10:18 10:00:00",
        "2026:10:18 10:01:00",
        "2026:10:18 10:02:00",
    ]
    .iter()
    .enumerate()
    {
        write(
            &card
                .path()
                .join(format!("DCIM/100_FUJI/DSCF000{}.JPG", n + 1)),
            &jpeg_taken(t),
        );
    }
    card
}

fn event_dir(lib: &Path) -> PathBuf {
    lib.join("2026").join("2026-10-18_Test-Day")
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

// --- the import job ---------------------------------------------------------

#[tokio::test]
async fn import_creates_folders_rows_previews() {
    let env = Env::new().await;
    let card = three_file_card();

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    let done = terminal(&frames);
    assert_eq!(done["type"], "import_done", "{frames:?}");
    assert_eq!(done["copied"], 3);
    assert_eq!(done["skipped_duplicates"], 0);
    assert_eq!(done["shoot_ids"].as_array().unwrap().len(), 1);

    let progress: Vec<&Value> = frames
        .iter()
        .filter(|f| f["type"] == "import_progress")
        .collect();
    assert!(!progress.is_empty(), "no progress frames in {frames:?}");
    assert!(progress.iter().all(|f| f["total"] == 3));
    // Progress counts only ever go up, and the last one names a real file.
    let dones: Vec<i64> = progress
        .iter()
        .map(|f| f["done"].as_i64().unwrap())
        .collect();
    assert!(dones.windows(2).all(|w| w[0] <= w[1]), "{dones:?}");
    assert_eq!(*dones.last().unwrap(), 3);
    assert!(progress
        .iter()
        .any(|f| f["current"].as_str().unwrap().starts_with("DSCF000")));

    let dir = event_dir(env.library.path());
    assert_eq!(
        names_in(&dir),
        ["DSCF0001.JPG", "DSCF0002.JPG", "DSCF0003.JPG"]
    );

    let db = env.db();
    assert_eq!(count(&db, "photos"), 3);
    assert_eq!(count(&db, "files"), 3);
    let rows = env.photos();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0].capture_time.unwrap().to_string(),
        "2026-10-18 10:00:00"
    );
    for row in &rows {
        assert_eq!((row.width, row.height), (Some(64), Some(48)));
        assert!(row.phash.is_some());
        assert_eq!(row.shoot_id, done["shoot_ids"][0].as_i64().unwrap());
        assert!(Path::new(row.jpeg_path.as_deref().unwrap()).starts_with(&dir));
        assert!(env.cache("preview", row.id).is_file(), "preview {}", row.id);
        assert!(env.cache("thumb", row.id).is_file(), "thumb {}", row.id);
    }
    let (folder, kind, name): (String, String, String) = db
        .query_row("SELECT folder, kind, name FROM shoots", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(Path::new(&folder), dir);
    assert_eq!((kind.as_str(), name.as_str()), ("named", "Test Day"));
}

#[tokio::test]
async fn reimport_same_card_copies_nothing() {
    let env = Env::new().await;
    let card = three_file_card();
    env.import_all(card.path(), "named", "Test Day").await;
    let dir = event_dir(env.library.path());
    let before = names_in(&dir);

    let rescan = env.scan(card.path()).await;
    assert_eq!(rescan["already_imported"], 3);

    let frames = env.import_all(card.path(), "named", "Test Day").await;
    let done = terminal(&frames);
    assert_eq!(done["type"], "import_done", "{frames:?}");
    assert_eq!(done["copied"], 0);
    assert_eq!(done["skipped_duplicates"], 3);
    assert_eq!(names_in(&dir), before);
    assert_eq!(count(&env.db(), "photos"), 3);
    assert_eq!(count(&env.db(), "shoots"), 1);
}

#[tokio::test]
async fn identical_files_within_one_card_are_deduped() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    let bytes = jpeg_taken("2026:10:18 10:00:00");
    write(&card.path().join("100_FUJI/DSCF0001.JPG"), &bytes);
    write(&card.path().join("101_FUJI/DSCF0001.JPG"), &bytes);

    let frames = env.import_all(card.path(), "named", "Test Day").await;
    let done = terminal(&frames);
    assert_eq!(done["copied"], 1, "{frames:?}");
    assert_eq!(done["skipped_duplicates"], 1);
    assert_eq!(count(&env.db(), "photos"), 1);
    assert_eq!(names_in(&event_dir(env.library.path())), ["DSCF0001.JPG"]);
}

#[tokio::test]
async fn interrupted_import_resumes_cleanly() {
    let env = Env::new().await;
    let card = three_file_card();
    // What a crash mid-event leaves behind: one file fully copied but never
    // recorded (rows are written at the end of an event), plus stale partials.
    let dir = event_dir(env.library.path());
    let first = std::fs::read(card.path().join("DCIM/100_FUJI/DSCF0001.JPG")).unwrap();
    write(&dir.join("DSCF0001.JPG"), &first);
    write(&dir.join("x.jpg.partial"), b"half a file");
    write(&dir.join("DSCF0002.JPG.partial"), b"half of another");

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    let done = terminal(&frames);
    assert_eq!(done["type"], "import_done", "{frames:?}");
    assert_eq!(
        names_in(&dir),
        ["DSCF0001.JPG", "DSCF0002.JPG", "DSCF0003.JPG"]
    );
    assert_eq!(count(&env.db(), "photos"), 3);
    assert_eq!(count(&env.db(), "files"), 3);
}

#[tokio::test]
async fn collision_keeps_raw_jpeg_pair_together() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    // RAF previews under 10 KB are treated as EXIF thumbnails, so go bigger.
    let jpeg = synthetic_jpeg_with_exif(
        1600,
        1200,
        Pattern::Blocks,
        &ExifSpec {
            make: Some("FUJIFILM"),
            model: Some("X100VI"),
            taken: Some("2026:10:18 10:00:00"),
            ..Default::default()
        },
    );
    write(&card.path().join("DSCF0001.RAF"), &fake_raf(&jpeg));
    write(&card.path().join("DSCF0001.JPG"), &jpeg);
    // An unrelated frame that happens to share the RAW's name.
    let dir = event_dir(env.library.path());
    write(&dir.join("DSCF0001.RAF"), b"somebody else's raw");

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");
    assert_eq!(
        names_in(&dir),
        ["DSCF0001.RAF", "DSCF0001_1.JPG", "DSCF0001_1.RAF"]
    );
    assert_eq!(
        std::fs::read(dir.join("DSCF0001.RAF")).unwrap(),
        b"somebody else's raw"
    );
    let (raw, jpg): (String, String) = env
        .db()
        .query_row("SELECT raw_path, jpeg_path FROM photos", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(Path::new(&raw), dir.join("DSCF0001_1.RAF"));
    assert_eq!(Path::new(&jpg), dir.join("DSCF0001_1.JPG"));
    let rows = env.photos();
    assert_eq!((rows[0].width, rows[0].height), (Some(1600), Some(1200)));
    assert!(env.cache("preview", rows[0].id).is_file());
    // The sibling JPEG is registered for dedupe too, under the same photo.
    assert_eq!(count(&env.db(), "files"), 2);
}

#[tokio::test]
async fn collision_on_the_jpeg_alone_moves_the_whole_pair() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    let jpeg = jpeg_taken("2026:10:18 10:00:00");
    write(&card.path().join("DSCF0001.RAF"), &fake_raf(&jpeg));
    write(&card.path().join("DSCF0001.JPG"), &jpeg);
    // Only the JPEG slot is taken, yet a RAW named _1 beside a JPEG named
    // plain would split the frame, so the RAW must follow.
    let dir = event_dir(env.library.path());
    write(&dir.join("DSCF0001.JPG"), b"somebody else's jpeg");

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");
    assert_eq!(
        names_in(&dir),
        ["DSCF0001.JPG", "DSCF0001_1.JPG", "DSCF0001_1.RAF"]
    );
}

#[tokio::test]
async fn same_name_items_in_one_import_get_distinct_names() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    // Cameras wrap their counters, so two folders often hold the same name.
    write(
        &card.path().join("100_FUJI/DSCF0001.JPG"),
        &jpeg_taken("2026:10:18 10:00:00"),
    );
    write(
        &card.path().join("101_FUJI/DSCF0001.JPG"),
        &jpeg_taken("2026:10:18 10:05:00"),
    );

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    assert_eq!(terminal(&frames)["copied"], 2, "{frames:?}");
    assert_eq!(
        names_in(&event_dir(env.library.path())),
        ["DSCF0001.JPG", "DSCF0001_1.JPG"]
    );
    assert_eq!(count(&env.db(), "photos"), 2);
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, (u64, SystemTime)> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, (u64, SystemTime)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            // Not DirEntry::metadata: on NTFS its directory mtimes can lag a
            // just-written entry, which would look like the card being touched.
            let meta = std::fs::metadata(e.path()).unwrap();
            out.insert(e.path(), (meta.len(), meta.modified().unwrap()));
            if meta.is_dir() {
                walk(&e.path(), out);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}

#[tokio::test]
async fn card_never_written() {
    let env = Env::new().await;
    let card = three_file_card();
    write(&card.path().join("DCIM/100_FUJI/CLIP0001.MOV"), b"movie");
    let before = snapshot(card.path());

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");
    assert_eq!(snapshot(card.path()), before);
}

#[tokio::test]
async fn video_copied_not_rendered() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    write(
        &card.path().join("DSCF0001.JPG"),
        &jpeg_taken("2026:10:18 10:00:00"),
    );
    let movie = card.path().join("CLIP0001.MOV");
    write(&movie, b"not really a movie, but bytes are bytes");

    let frames = env.import_all(card.path(), "named", "Test Day").await;
    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");

    let dir = event_dir(env.library.path());
    assert_eq!(names_in(&dir), ["CLIP0001.MOV", "DSCF0001.JPG"]);
    let db = env.db();
    let row = env
        .photos()
        .into_iter()
        .find(|p| p.video_path.is_some())
        .expect("a row for the movie");
    assert_eq!(
        Path::new(row.video_path.as_deref().unwrap()),
        dir.join("CLIP0001.MOV")
    );
    assert!(row.raw_path.is_none() && row.jpeg_path.is_none());
    assert!(row.width.is_none() && row.phash.is_none());
    let id = row.id;
    assert!(!env.cache("preview", id).exists());
    assert!(!env.cache("thumb", id).exists());
    // Videos are deduped by content like everything else.
    assert_eq!(count(&db, "files"), 2);
}

#[tokio::test]
async fn camera_offset_shifts_capture_time_and_is_stored() {
    let env = Env::new().await;
    let card = three_file_card();

    let frames = env
        .import_all_with(
            card.path(),
            "named",
            "Test Day",
            json!({ "FUJIFILM X100VI": 3600 }),
        )
        .await;

    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");
    let db = env.db();
    let (orig, shifted): (String, String) = db
        .query_row(
            "SELECT capture_time_orig, capture_time FROM photos ORDER BY capture_time LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(orig, "2026-10-18T10:00:00");
    assert_eq!(shifted, "2026-10-18T11:00:00");
    let (camera, secs): (String, i64) = db
        .query_row(
            "SELECT camera, offset_seconds FROM camera_offsets",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((camera.as_str(), secs), ("FUJIFILM X100VI", 3600));
}

#[tokio::test]
async fn everyday_imports_share_the_month_shoot() {
    let env = Env::new().await;
    let first = tempfile::tempdir().unwrap();
    write(
        &first.path().join("DSCF0001.JPG"),
        &jpeg_taken("2026:10:03 09:00:00"),
    );
    let second = tempfile::tempdir().unwrap();
    write(
        &second.path().join("DSCF0002.JPG"),
        &jpeg_taken("2026:10:21 18:00:00"),
    );

    let a = env.import_all(first.path(), "everyday", "").await;
    let b = env.import_all(second.path(), "everyday", "").await;

    assert_eq!(terminal(&a)["type"], "import_done", "{a:?}");
    assert_eq!(terminal(&b)["type"], "import_done", "{b:?}");
    assert_eq!(terminal(&a)["shoot_ids"], terminal(&b)["shoot_ids"]);
    let db = env.db();
    assert_eq!(count(&db, "shoots"), 1);
    assert_eq!(count(&db, "photos"), 2);
    let folder: String = db
        .query_row("SELECT folder FROM shoots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        Path::new(&folder),
        env.library.path().join("2026").join("2026-10_Everyday")
    );
    assert_eq!(
        names_in(Path::new(&folder)),
        ["DSCF0001.JPG", "DSCF0002.JPG"]
    );
}

#[tokio::test]
async fn unknown_scan_and_bad_indexes_are_rejected() {
    let env = Env::new().await;
    let card = three_file_card();
    let scan = env.scan(card.path()).await;

    let body = |scan_id: &Value, idxs: Value| {
        json!({
            "scan_id": scan_id,
            "events": [{ "item_idxs": idxs, "kind": "named", "name": "X" }],
            "camera_offsets": {},
        })
    };
    let unknown = env
        .http
        .post(env.url("/api/import"))
        .json(&body(&json!("nope"), json!([0])))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
    let out_of_range = env
        .http
        .post(env.url("/api/import"))
        .json(&body(&scan["scan_id"], json!([0, 99])))
        .send()
        .await
        .unwrap();
    assert_eq!(out_of_range.status(), 400);
    let duplicate = env
        .http
        .post(env.url("/api/import"))
        .json(&body(&scan["scan_id"], json!([0, 0])))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 400);
    // Nothing was started, so nothing may have been created.
    assert!(!env.library.path().join("2026").exists());
}

#[tokio::test]
async fn scan_endpoint_reports_unreadable_source() {
    let env = Env::new().await;
    let resp = env
        .http
        .post(env.url("/api/scan"))
        .json(&json!({ "source": env.library.path().join("missing") }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn extra_same_stem_jpeg_is_imported() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    let jpeg = jpeg_taken("2026:10:18 10:00:00");
    write(&card.path().join("A.RAF"), &fake_raf(&jpeg));
    write(
        &card.path().join("A.JPG"),
        &jpeg_taken("2026:10:18 10:00:01"),
    );
    write(
        &card.path().join("A.JPEG"),
        &jpeg_taken("2026:10:18 10:00:02"),
    );

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");
    assert_eq!(names_in(&event_dir(env.library.path())).len(), 3);
    assert_eq!(count(&env.db(), "photos"), 2);
    assert_eq!(count(&env.db(), "files"), 3);
}

// --- fix round 1 -------------------------------------------------------------

/// Large enough that a RAF's embedded JPEG passes the 10 KB thumbnail cutoff.
fn big_jpeg(taken: &'static str, pattern: Pattern) -> Vec<u8> {
    synthetic_jpeg_with_exif(
        1600,
        1200,
        pattern,
        &ExifSpec {
            make: Some("FUJIFILM"),
            model: Some("X100VI"),
            taken: Some(taken),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn late_sibling_jpeg_attaches_to_the_already_imported_raw() {
    let env = Env::new().await;
    let raw_jpeg = big_jpeg("2026:10:18 10:00:00", Pattern::Blocks);
    let raf = fake_raf(&raw_jpeg);

    // Day 1: only the RAW was imported (say from a RAW-only card).
    let first = tempfile::tempdir().unwrap();
    write(&first.path().join("DSCF0001.RAF"), &raf);
    let frames = env.import_all(first.path(), "named", "Test Day").await;
    assert_eq!(terminal(&frames)["copied"], 1, "{frames:?}");

    // Later: the same RAW arrives again, now with its camera JPEG.
    let second = tempfile::tempdir().unwrap();
    write(&second.path().join("DSCF0001.RAF"), &raf);
    let camera_jpeg = big_jpeg("2026:10:18 10:00:00", Pattern::Stripes);
    write(&second.path().join("DSCF0001.JPG"), &camera_jpeg);
    let frames = env.import_all(second.path(), "named", "Test Day").await;

    let done = terminal(&frames);
    assert_eq!(done["type"], "import_done", "{frames:?}");
    assert_eq!(done["copied"], 1, "the JPEG is a copy");
    assert_eq!(done["skipped_duplicates"], 0);
    let dir = event_dir(env.library.path());
    assert_eq!(names_in(&dir), ["DSCF0001.JPG", "DSCF0001.RAF"]);
    assert_eq!(
        std::fs::read(dir.join("DSCF0001.JPG")).unwrap(),
        camera_jpeg
    );

    let rows = env.photos();
    assert_eq!(rows.len(), 1, "attached, not a second photo");
    assert_eq!(
        Path::new(rows[0].jpeg_path.as_deref().unwrap()),
        dir.join("DSCF0001.JPG")
    );
    assert_eq!(count(&env.db(), "files"), 2);
    let owner: i64 = env
        .db()
        .query_row(
            "SELECT photo_id FROM files WHERE path = ?1",
            [dir.join("DSCF0001.JPG").to_string_lossy().into_owned()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(owner, rows[0].id);

    // Importing that card a third time finds everything and copies nothing.
    let frames = env.import_all(second.path(), "named", "Test Day").await;
    assert_eq!(terminal(&frames)["copied"], 0, "{frames:?}");
    assert_eq!(terminal(&frames)["skipped_duplicates"], 1);
}

#[tokio::test]
async fn extra_jpeg_for_a_raw_that_already_has_one_becomes_its_own_photo() {
    let env = Env::new().await;
    let raf = fake_raf(&big_jpeg("2026:10:18 10:00:00", Pattern::Blocks));
    let first = tempfile::tempdir().unwrap();
    write(&first.path().join("DSCF0001.RAF"), &raf);
    write(
        &first.path().join("DSCF0001.JPG"),
        &big_jpeg("2026:10:18 10:00:00", Pattern::Stripes),
    );
    env.import_all(first.path(), "named", "Test Day").await;

    // A different JPEG for the same RAW: nothing to attach to, nothing to lose.
    let second = tempfile::tempdir().unwrap();
    write(&second.path().join("DSCF0001.RAF"), &raf);
    write(
        &second.path().join("DSCF0001.JPG"),
        &big_jpeg("2026:10:18 10:00:05", Pattern::Stripes),
    );
    let frames = env.import_all(second.path(), "named", "Test Day").await;

    assert_eq!(terminal(&frames)["copied"], 1, "{frames:?}");
    let dir = event_dir(env.library.path());
    assert_eq!(
        names_in(&dir),
        ["DSCF0001.JPG", "DSCF0001.RAF", "DSCF0001_1.JPG"]
    );
    let rows = env.photos();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r.raw_path.is_none()
        && Path::new(r.jpeg_path.as_deref().unwrap()) == dir.join("DSCF0001_1.JPG")));
}

#[tokio::test]
async fn everyday_event_spanning_two_months_is_rejected() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    write(
        &card.path().join("DSCF0001.JPG"),
        &jpeg_taken("2026:10:31 23:30:00"),
    );
    write(
        &card.path().join("DSCF0002.JPG"),
        &jpeg_taken("2026:11:02 09:00:00"),
    );
    let scan = env.scan(card.path()).await;
    let post = |offsets: Value| {
        env.http
            .post(env.url("/api/import"))
            .json(&json!({
                "scan_id": scan["scan_id"],
                "events": [{ "item_idxs": [0, 1], "kind": "everyday", "name": "" }],
                "camera_offsets": offsets,
            }))
            .send()
    };

    let resp = post(json!({})).await.unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("month"), "{body}");
    assert!(!env.library.path().join("2026").exists());

    // Offset-corrected times decide: +1h moves the first frame into November.
    let mut ws = env.ws().await;
    let resp = post(json!({ "FUJIFILM X100VI": 3600 })).await.unwrap();
    assert_eq!(resp.status(), 200);
    let job_id = resp.json::<Value>().await.unwrap()["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    let frames = collect_until_terminal(&mut ws, &job_id).await;
    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");
    assert!(env
        .library
        .path()
        .join("2026")
        .join("2026-11_Everyday")
        .is_dir());
}

#[tokio::test]
async fn resumed_pair_stays_together_even_with_a_sidecar_present() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    let raf = fake_raf(&big_jpeg("2026:10:18 10:00:00", Pattern::Blocks));
    let jpeg = big_jpeg("2026:10:18 10:00:00", Pattern::Stripes);
    write(&card.path().join("DSCF0001.RAF"), &raf);
    write(&card.path().join("DSCF0001.JPG"), &jpeg);
    // A crash left the RAW fully copied, and a sidecar sits beside it.
    let dir = event_dir(env.library.path());
    write(&dir.join("DSCF0001.RAF"), &raf);
    write(&dir.join("DSCF0001.xmp"), b"<x:xmpmeta/>");

    let frames = env.import_all(card.path(), "named", "Test Day").await;

    assert_eq!(terminal(&frames)["type"], "import_done", "{frames:?}");
    assert_eq!(
        names_in(&dir),
        ["DSCF0001.JPG", "DSCF0001.RAF", "DSCF0001.xmp"]
    );
    let rows = env.photos();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        Path::new(rows[0].jpeg_path.as_deref().unwrap()),
        dir.join("DSCF0001.JPG")
    );
}
