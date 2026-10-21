//! End-to-end check against a real-camera card built by
//! `scripts/make-golden-fixture.ps1`. The fixture lives outside git (public
//! repo); see tests/golden/README.md.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use photosift::db::photos::{self, PhotoRow};
use serde_json::{json, Value};

/// Stems of the portrait shots in the golden set. Their previews must come
/// out taller than wide, which only holds if EXIF orientation was applied.
const PORTRAIT_STEMS: &[&str] = &["DSCF1654", "DSCF1890", "DSCF1921"];

fn golden_dir() -> PathBuf {
    // Panic rather than skip: an --ignored run that checked nothing must not look green.
    std::env::var_os("PHOTOSIFT_GOLDEN_DIR")
        .map(PathBuf::from)
        .expect("set PHOTOSIFT_GOLDEN_DIR; see tests/golden/README.md")
}

struct Expected {
    photos: usize,
    cameras: usize,
    events: usize,
}

fn read_expected(dir: &Path) -> Expected {
    let path = dir.join("expected.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path:?}: {e}; run scripts/make-golden-fixture.ps1"));
    let table: toml::Table = text.parse().expect("expected.toml parses");
    let get = |k: &str| {
        table[k]
            .as_integer()
            .unwrap_or_else(|| panic!("expected.toml: {k} missing")) as usize
    };
    Expected {
        photos: get("photos"),
        cameras: get("cameras"),
        events: get("events"),
    }
}

fn jpeg_dims(path: &Path) -> (u32, u32) {
    let bytes = std::fs::read(path).unwrap();
    let mut d = zune_jpeg::JpegDecoder::new(&bytes);
    d.decode_headers().expect("preview jpeg headers");
    let info = d.info().expect("preview jpeg info");
    (u32::from(info.width), u32::from(info.height))
}

fn stem(p: &Option<String>) -> Option<String> {
    p.as_deref()
        .and_then(|p| Path::new(p).file_stem())
        .map(|s| s.to_string_lossy().into_owned())
}

#[tokio::test]
#[ignore = "needs PHOTOSIFT_GOLDEN_DIR (real fixture; run scripts/make-golden-fixture.ps1)"]
async fn golden_card_scans_and_imports() {
    let dir = golden_dir();
    let expected = read_expected(&dir);

    let library = tempfile::tempdir().unwrap();
    let server = common::spawn_server_with_library(library.path()).await;
    let http = reqwest::Client::new();
    let url = |p: &str| format!("{}{p}", server.base_url);

    // --- scan ---
    let resp = http
        .post(url("/api/scan"))
        .json(&json!({ "source": dir }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let scan: Value = resp.json().await.unwrap();
    let items = scan["items"].as_array().unwrap();
    assert_eq!(items.len(), expected.photos, "scanned item count");
    assert_eq!(
        scan["cameras"].as_array().unwrap().len(),
        expected.cameras,
        "camera count"
    );
    assert_eq!(
        scan["events"].as_array().unwrap().len(),
        expected.events,
        "proposed event count"
    );
    assert_eq!(scan["warnings"].as_array().unwrap().len(), 0, "{scan}");
    // The scan's full-read fallback must have found a real EXIF time everywhere.
    for item in items {
        assert_eq!(item["time_source"], "exif", "{}", item["camera"]);
    }

    // --- import every proposed event ---
    let events: Vec<Value> = scan["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            json!({
                "item_idxs": e["item_idxs"],
                "kind": e["kind"],
                "name": if e["kind"] == "named" { json!("Golden Test") } else { json!("") },
            })
        })
        .collect();
    let url_ws = url("/ws").replacen("http://", "ws://", 1);
    let (mut ws, _) = tokio_tungstenite::connect_async(url_ws).await.unwrap();
    let resp = http
        .post(url("/api/import"))
        .json(&json!({
            "scan_id": scan["scan_id"],
            "events": events,
            "camera_offsets": {},
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let job_id = resp.json::<Value>().await.unwrap()["job_id"]
        .as_str()
        .expect("job_id")
        .to_string();

    let terminal = tokio::time::timeout(Duration::from_secs(300), async {
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
    .expect("import did not finish in time");
    assert_eq!(terminal["type"], "import_done", "{terminal}");
    assert_eq!(terminal["copied"], expected.photos);

    // --- every photo has a preview and a pHash ---
    let pool = photosift::db::open(&server.data_root.path().join("photosift.db")).unwrap();
    let ids: Vec<i64> = {
        let db = rusqlite::Connection::open(server.data_root.path().join("photosift.db")).unwrap();
        let mut stmt = db.prepare("SELECT id FROM photos ORDER BY id").unwrap();
        let ids = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        ids
    };
    assert_eq!(ids.len(), expected.photos, "photos rows");
    let rows: Vec<PhotoRow> = ids
        .iter()
        .map(|id| photos::get_photo(&pool, *id).unwrap().unwrap())
        .collect();

    let mut portraits_seen = 0;
    for row in &rows {
        let preview = server
            .data_root
            .path()
            .join("cache")
            .join("preview")
            .join(format!("{}.jpg", row.id));
        assert!(preview.exists(), "photo {}: no preview", row.id);
        assert!(row.phash.is_some(), "photo {}: no phash", row.id);
        let (w, h) = (row.width.unwrap(), row.height.unwrap());
        let (pw, ph) = jpeg_dims(&preview);
        assert_eq!((w >= h), pw >= ph, "photo {}", row.id);

        let name = stem(&row.raw_path).or_else(|| stem(&row.jpeg_path));
        if name.as_deref().is_some_and(|n| PORTRAIT_STEMS.contains(&n)) {
            portraits_seen += 1;
            assert!(ph > pw, "{name:?}: preview {}x{} is not portrait", pw, ph);
            assert!(h > w, "{name:?}: stored dims {w}x{h} are not portrait");
        }
    }
    assert_eq!(portraits_seen, PORTRAIT_STEMS.len(), "portraits imported");
}
