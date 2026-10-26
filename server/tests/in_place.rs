//! `POST /api/import/in-place`: a library folder PhotoSift did not create
//! (e.g. one Lightroom imported) opened as a shoot without copying anything.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{fake_raf, spawn_server_with_library, synthetic_jpeg_with_exif, ExifSpec, Pattern};
use futures_util::StreamExt;
use rusqlite::Connection;
use serde_json::{json, Value};
use tempfile::TempDir;

/// A Lightroom-made sidecar: its develop settings must survive our write.
const LIGHTROOM_SIDECAR: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    crs:Exposure2012="+0.35"/>
 </rdf:RDF>
</x:xmpmeta>
"#;

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

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let resp = self
            .http
            .post(self.url(path))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        (status, resp.json().await.unwrap_or(Value::Null))
    }

    async fn open(&self, folder: &Path) -> (u16, Value) {
        self.post("/api/import/in-place", json!({ "folder": folder }))
            .await
    }

    /// Opens `folder` (which must be new) and waits for the job's terminal frame.
    async fn open_new(&self, folder: &Path) -> (Value, Value) {
        let url = self.url("/ws").replacen("http://", "ws://", 1);
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let (status, reply) = self.open(folder).await;
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply["status"], "indexing", "{reply}");
        let job_id = reply["job_id"].as_str().unwrap().to_string();
        let done = tokio::time::timeout(Duration::from_secs(60), async {
            while let Some(msg) = ws.next().await {
                let Ok(text) = msg.unwrap().into_text() else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if v["job_id"] == job_id.as_str()
                    && matches!(v["type"].as_str(), Some("import_done" | "import_failed"))
                {
                    return v;
                }
            }
            panic!("websocket closed before the job finished");
        })
        .await
        .expect("indexing did not finish in time");
        (reply, done)
    }
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

/// Every path under `dir`, relative, sorted.
fn tree(dir: &Path) -> Vec<PathBuf> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            out.push(path.strip_prefix(base).unwrap().to_path_buf());
            if path.is_dir() {
                walk(base, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// A shoot folder as Lightroom leaves it (not PhotoSift's `YYYY-MM-DD_` name):
/// a RAF with Lightroom's sidecar, two JPEGs, a clip and an `Export\` of edits.
fn lightroom_folder(library: &Path) -> PathBuf {
    let folder = library.join("2024").join("2024-05_Trip");
    write(
        &folder.join("DSCF0001.RAF"),
        &fake_raf(&jpeg_taken("2024:05:04 10:00:00")),
    );
    write(&folder.join("DSCF0001.xmp"), LIGHTROOM_SIDECAR.as_bytes());
    write(
        &folder.join("DSCF0002.JPG"),
        &jpeg_taken("2024:05:04 10:01:00"),
    );
    write(
        &folder.join("DSCF0003.JPG"),
        &jpeg_taken("2024:05:04 10:02:00"),
    );
    write(&folder.join("CLIP0004.MOV"), b"not really a movie");
    write(
        &folder.join("Export").join("DSCF0002-edit.jpg"),
        &jpeg_taken("2024:05:04 10:01:00"),
    );
    folder
}

#[tokio::test]
async fn opens_a_library_folder_in_place_and_writes_sidecars_beside_the_originals() {
    let env = Env::new().await;
    let folder = lightroom_folder(env.library.path());
    let library_before = tree(env.library.path());

    let (reply, done) = env.open_new(&folder).await;
    assert_eq!(reply["photos"], 3, "{reply}");
    assert_eq!(done["type"], "import_done", "{done}");
    let shoot_ids = done["shoot_ids"].as_array().unwrap();
    assert_eq!(shoot_ids.len(), 1, "{done}");
    let shoot_id = shoot_ids[0].as_i64().unwrap();

    // Nothing copied, created or touched in the library.
    assert_eq!(tree(env.library.path()), library_before);

    let db = env.db();
    let (name, shoot_folder): (String, String) = db
        .query_row(
            "SELECT name, folder FROM shoots WHERE id = ?1",
            [shoot_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(name, "2024-05_Trip");
    assert_eq!(
        Path::new(&shoot_folder).canonicalize().unwrap(),
        folder.canonicalize().unwrap()
    );
    // Rows point at the originals; no video and nothing from Export\.
    let mut stmt = db
        .prepare("SELECT id, raw_path, jpeg_path, video_path FROM photos ORDER BY capture_time")
        .unwrap();
    type Row = (i64, Option<String>, Option<String>, Option<String>);
    let rows: Vec<Row> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(rows.iter().all(|r| r.3.is_none()), "{rows:?}");
    let raw = PathBuf::from(rows[0].1.as_deref().expect("the RAF is a RAW"));
    assert_eq!(
        raw.canonicalize().unwrap(),
        folder.join("DSCF0001.RAF").canonicalize().unwrap()
    );
    for (row, file) in rows[1..].iter().zip(["DSCF0002.JPG", "DSCF0003.JPG"]) {
        let jpeg = PathBuf::from(row.2.as_deref().unwrap());
        assert_eq!(
            jpeg.canonicalize().unwrap(),
            folder.join(file).canonicalize().unwrap()
        );
    }
    let preview = env.server.data_root.path().join("cache").join("preview");
    assert!(preview.join(format!("{}.jpg", rows[1].0)).is_file());

    // Culling writes sidecars beside the originals, keeping Lightroom's fields.
    let (status, body) = env
        .post(
            &format!("/api/photos/{}/outcome", rows[0].0),
            json!({ "outcome": "album" }),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let xml = std::fs::read_to_string(folder.join("DSCF0001.xmp")).unwrap();
    assert!(xml.contains("xmp:Rating=\"4\""), "{xml}");
    assert!(xml.contains("crs:Exposure2012=\"+0.35\""), "{xml}");

    let (status, body) = env
        .post(
            &format!("/api/photos/{}/outcome", rows[1].0),
            json!({ "outcome": "library" }),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let xml = std::fs::read_to_string(folder.join("DSCF0002.xmp")).unwrap();
    assert!(xml.contains("xmp:Rating=\"3\""), "{xml}");

    // Re-opening, however the path is spelled, lands on the same shoot.
    let respelled = PathBuf::from(folder.to_string_lossy().to_uppercase())
        .join("..")
        .join("2024-05_Trip");
    for again in [folder.clone(), respelled] {
        let (status, reply) = env.open(&again).await;
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply, json!({ "status": "existing", "shoot_id": shoot_id }));
    }
    let shoots: i64 = db
        .query_row("SELECT COUNT(*) FROM shoots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(shoots, 1);
}

#[tokio::test]
async fn refuses_folders_outside_the_library_and_the_library_itself() {
    let env = Env::new().await;
    let outside = tempfile::tempdir().unwrap();
    write(
        &outside.path().join("DSCF0001.JPG"),
        &jpeg_taken("2024:05:04 10:00:00"),
    );
    let inside = env.library.path().join("2024").join("2024-05_Trip");
    std::fs::create_dir_all(&inside).unwrap();
    let climbing = inside.join("..").join("..").join("..");

    for folder in [
        outside.path().to_path_buf(),
        env.library.path().to_path_buf(),
        climbing,
        env.library.path().join("missing"),
        PathBuf::from("relative\\folder"),
        PathBuf::from(r"\\host\share\photos"),
    ] {
        let (status, body) = env.open(&folder).await;
        assert_eq!(status, 400, "{folder:?}: {body}");
        assert!(body["error"].is_string(), "{body}");
    }
    // An empty folder inside the library has nothing to open.
    let (status, body) = env.open(&inside).await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("no photos"),
        "{body}"
    );

    let db = env.db();
    let rows: i64 = db
        .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(tree(outside.path()), [PathBuf::from("DSCF0001.JPG")]);
}

#[tokio::test]
async fn a_folder_photosift_imported_into_opens_as_its_shoot() {
    let env = Env::new().await;
    let card = tempfile::tempdir().unwrap();
    write(
        &card.path().join("DCIM/100_FUJI/DSCF0001.JPG"),
        &jpeg_taken("2026:10:18 10:00:00"),
    );
    let (status, scan) = env
        .post("/api/scan", json!({ "source": card.path() }))
        .await;
    assert_eq!(status, 200, "{scan}");
    let url = env.url("/ws").replacen("http://", "ws://", 1);
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let (status, started) = env
        .post(
            "/api/import",
            json!({
                "scan_id": scan["scan_id"],
                "events": [{ "item_idxs": [0], "kind": "named", "name": "Test Day" }],
            }),
        )
        .await;
    assert_eq!(status, 200, "{started}");
    let done = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(msg) = ws.next().await {
            let Ok(text) = msg.unwrap().into_text() else {
                continue;
            };
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if v["job_id"] == started["job_id"] && v["type"] == "import_done" {
                return v;
            }
        }
        panic!("websocket closed before the import finished");
    })
    .await
    .unwrap();
    let shoot_id = done["shoot_ids"][0].as_i64().unwrap();

    let folder = env.library.path().join("2026").join("2026-10-18_Test-Day");
    let (status, reply) = env.open(&folder).await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply, json!({ "status": "existing", "shoot_id": shoot_id }));
}
