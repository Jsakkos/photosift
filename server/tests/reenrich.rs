mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{respawn_server, spawn_server_with_library, TestServer};
use futures_util::StreamExt;
use photosift::db::photos;
use photosift::enrich::FACES_MODEL;
use serde_json::{json, Value};

const MODEL_FILES: [&str; 2] = [
    "face_detection_yunet_2023mar.onnx",
    "face_recognition_sface_2021dec.onnx",
];

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn open_db(root: &Path) -> photosift::db::DbPool {
    photosift::db::open(&root.join("photosift.db")).unwrap()
}

fn photo_ids(pool: &photosift::db::DbPool) -> Vec<i64> {
    pool.with(|c| {
        let mut stmt = c.prepare("SELECT id FROM photos ORDER BY id")?;
        let ids = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>();
        ids
    })
    .unwrap()
}

fn faces_model(pool: &photosift::db::DbPool, id: i64) -> Option<String> {
    pool.with(|c| {
        c.query_row("SELECT faces_model FROM photos WHERE id = ?1", [id], |r| {
            r.get(0)
        })
    })
    .unwrap()
}

async fn status(base: &str) -> Value {
    reqwest::get(format!("{base}/api/status"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn wait_for_empty_queue(base: &str) {
    for _ in 0..600 {
        if status(base).await["queue"] == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("queue never drained: {}", status(base).await);
}

async fn connect_ws(base: &str) -> Ws {
    let url = format!("{base}/ws").replacen("http://", "ws://", 1);
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

/// Imports everything under `source` and waits for every touched shoot to
/// report `enrich_done`.
async fn import_and_enrich(base: &str, source: &Path) {
    let http = reqwest::Client::new();
    let scan: Value = http
        .post(format!("{base}/api/scan"))
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
                "name": if e["kind"] == "named" { json!("Reenrich Test") } else { json!("") },
            })
        })
        .collect();
    let mut ws = connect_ws(base).await;
    let resp = http
        .post(format!("{base}/api/import"))
        .json(&json!({ "scan_id": scan["scan_id"], "events": events, "camera_offsets": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    wait_for_enrich_done(&mut ws, None).await;
}

/// Waits for `enrich_done` of every shoot in `shoots`, or, when `None`, of
/// every shoot named by the next `import_done`.
async fn wait_for_enrich_done(ws: &mut Ws, shoots: Option<Vec<i64>>) {
    let mut wanted = shoots;
    let mut done: HashSet<i64> = HashSet::new();
    tokio::time::timeout(Duration::from_secs(600), async {
        while let Some(msg) = ws.next().await {
            let Ok(text) = msg.unwrap().into_text() else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            match v["type"].as_str() {
                Some("import_done") => {
                    wanted = Some(
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
                    done.insert(v["shoot_id"].as_i64().unwrap());
                }
                _ => {}
            }
            if let Some(w) = &wanted {
                if w.iter().all(|id| done.contains(id)) {
                    return;
                }
            }
        }
        panic!("websocket closed early");
    })
    .await
    .expect("enrichment did not finish in time");
}

fn golden_dir() -> PathBuf {
    // Panic rather than skip: an --ignored run that checked nothing must not look green.
    std::env::var_os("PHOTOSIFT_GOLDEN_DIR")
        .map(PathBuf::from)
        .expect("set PHOTOSIFT_GOLDEN_DIR; see tests/golden/README.md")
}

fn models_dir() -> PathBuf {
    std::env::var_os("PHOTOSIFT_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| photosift::config::data_root().join("models"))
}

fn install_models(server: &TestServer) {
    let dest = server.data_root.path().join("models");
    std::fs::create_dir_all(&dest).unwrap();
    for name in MODEL_FILES {
        std::fs::copy(models_dir().join(name), dest.join(name))
            .unwrap_or_else(|e| panic!("{name}: {e}; run scripts/fetch-models.ps1"));
    }
}

fn synthetic_card() -> tempfile::TempDir {
    let card = tempfile::tempdir().unwrap();
    for (n, t) in ["2026:10:18 10:00:00", "2026:10:18 10:01:00"]
        .iter()
        .enumerate()
    {
        let path = card
            .path()
            .join(format!("DCIM/100_FUJI/DSCF000{}.JPG", n + 1));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            common::synthetic_jpeg_with_exif(
                640,
                480,
                common::Pattern::Stripes,
                &common::ExifSpec {
                    make: Some("FUJIFILM"),
                    model: Some("X100VI"),
                    taken: Some(t),
                    ..Default::default()
                },
            ),
        )
        .unwrap();
    }
    card
}

#[tokio::test]
async fn no_reenrich_without_models() {
    let library = tempfile::tempdir().unwrap();
    let server = spawn_server_with_library(library.path()).await;
    let card = synthetic_card();
    import_and_enrich(&server.base_url, card.path()).await;
    wait_for_empty_queue(&server.base_url).await;

    let pool = open_db(server.data_root.path());
    let ids = photo_ids(&pool);
    assert_eq!(ids.len(), 2);
    for id in &ids {
        assert_eq!(faces_model(&pool, *id), None, "no detector ran");
    }
    assert_eq!(
        photos::shoots_needing_faces(&pool, FACES_MODEL)
            .unwrap()
            .len(),
        1,
        "the shoot is stale in the db"
    );
    drop(pool);

    // Still no models after a restart: nothing is queued.
    let server = respawn_server(server).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(status(&server.base_url).await["queue"], 0);
}

#[tokio::test]
#[ignore = "needs PHOTOSIFT_GOLDEN_DIR and the face models (scripts/fetch-models.ps1)"]
async fn faces_added_after_models_arrive() {
    let library = tempfile::tempdir().unwrap();
    let server = spawn_server_with_library(library.path()).await;
    import_and_enrich(&server.base_url, &golden_dir()).await;

    let pool = open_db(server.data_root.path());
    let ids = photo_ids(&pool);
    assert!(!ids.is_empty());
    let before: Vec<photos::PhotoRow> = ids
        .iter()
        .map(|id| photos::get_photo(&pool, *id).unwrap().unwrap())
        .collect();
    for id in &ids {
        assert!(photos::list_faces(&pool, *id).unwrap().is_empty());
        assert_eq!(faces_model(&pool, *id), None);
    }
    drop(pool);

    install_models(&server);
    let server = respawn_server(server).await;
    let st = status(&server.base_url).await;
    assert_ne!(
        st["ai_backend"], "unavailable",
        "models failed to load: {st}"
    );
    // Polling the queue is enough: the worker has the shoot queued from start.
    tokio::time::sleep(Duration::from_millis(200)).await;
    wait_for_empty_queue(&server.base_url).await;

    let pool = open_db(server.data_root.path());
    let mut total_faces = 0;
    let mut score_changed = false;
    for old in &before {
        let new = photos::get_photo(&pool, old.id).unwrap().unwrap();
        total_faces += photos::list_faces(&pool, old.id).unwrap().len();
        assert_eq!(faces_model(&pool, old.id).as_deref(), Some(FACES_MODEL));
        assert_eq!(new.sharpness, old.sharpness, "photo {}: sharpness", old.id);
        score_changed |= new.signal_score != old.signal_score;
    }
    assert!(total_faces > 0, "no faces found after the models arrived");
    assert!(score_changed, "signal_score never changed");
}
