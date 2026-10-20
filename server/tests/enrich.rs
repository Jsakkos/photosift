mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{spawn_server, spawn_server_with_library, spawn_server_with_models};
use futures_util::StreamExt;
use photosift::db::photos;
use photosift::enrich::{
    faces::Detector,
    identity::{similarity, ARCFACE_TEMPLATE},
    quality::measure,
};
use photosift::media::render;
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

// ---------------------------------------------------------------- pure units

fn checkerboard(w: usize, h: usize, cell: usize) -> Vec<f32> {
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            if (x / cell + y / cell).is_multiple_of(2) {
                230.0
            } else {
                20.0
            }
        })
        .collect()
}

fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut out = vec![0.0; src.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n) = (0.0, 0.0);
            for yy in y.saturating_sub(r)..=(y + r).min(h - 1) {
                for xx in x.saturating_sub(r)..=(x + r).min(w - 1) {
                    sum += src[yy * w + xx];
                    n += 1.0;
                }
            }
            out[y * w + x] = sum / n;
        }
    }
    out
}

#[test]
fn quality_blur_scores_lower() {
    let (w, h) = (96, 64);
    let sharp = checkerboard(w, h, 4);
    let blurred = box_blur(&sharp, w, h, 3);
    let s = measure(&sharp, w, h, &[]);
    let b = measure(&blurred, w, h, &[]);
    assert!(
        s.sharpness > b.sharpness,
        "sharp {} must beat blurred {}",
        s.sharpness,
        b.sharpness
    );
    assert!(s.face_sharpness.is_none(), "no faces, no face sharpness");
}

#[test]
fn quality_flags_clipped_exposure() {
    let white = vec![255.0f32; 64 * 48];
    let q = measure(&white, 64, 48, &[]);
    assert!(q.clip_hi > 0.99, "clip_hi {}", q.clip_hi);
    assert!(q.clip_lo < 0.01);
    assert!((q.mean_luma - 255.0).abs() < 1e-6);

    let black = vec![0.0f32; 64 * 48];
    let q = measure(&black, 64, 48, &[]);
    assert!(q.clip_lo > 0.99, "clip_lo {}", q.clip_lo);
    assert!(q.clip_hi < 0.01);
}

#[test]
fn similarity_maps_template_to_itself() {
    let m = similarity(&ARCFACE_TEMPLATE);
    assert!((m[0] - 1.0).abs() < 1e-4, "a = {}", m[0]);
    assert!(m[3].abs() < 1e-4, "b = {}", m[3]);
    assert!(m[1].abs() < 1e-4 && (m[4] - 1.0).abs() < 1e-4);
    assert!(m[2].abs() < 1e-3, "tx = {}", m[2]);
    assert!(m[5].abs() < 1e-3, "ty = {}", m[5]);
}

#[test]
fn similarity_recovers_scale_and_translation() {
    // Landmarks that are the template scaled 2x and shifted: the transform
    // must undo it, i.e. map each landmark back to the template.
    let src = ARCFACE_TEMPLATE.map(|(x, y)| (x * 2.0 + 100.0, y * 2.0 + 50.0));
    let m = similarity(&src);
    for (s, d) in src.iter().zip(ARCFACE_TEMPLATE.iter()) {
        let x = m[0] * s.0 + m[1] * s.1 + m[2];
        let y = m[3] * s.0 + m[4] * s.1 + m[5];
        assert!(
            (x - d.0).abs() < 1e-2 && (y - d.1).abs() < 1e-2,
            "{x},{y} vs {d:?}"
        );
    }
}

// ---------------------------------------------------------------- end to end

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn jpeg_taken(taken: &'static str) -> Vec<u8> {
    common::synthetic_jpeg_with_exif(
        640,
        480,
        common::Pattern::Stripes,
        &common::ExifSpec {
            make: Some("FUJIFILM"),
            model: Some("X100VI"),
            taken: Some(taken),
            ..Default::default()
        },
    )
}

fn three_file_card() -> tempfile::TempDir {
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

async fn connect_ws(base: &str) -> Ws {
    let url = format!("{base}/ws").replacen("http://", "ws://", 1);
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

struct Imported {
    shoot_ids: Vec<i64>,
    /// Time from the import finishing to the last shoot being enriched.
    enrich_wall: Duration,
}

/// Scans `source`, imports every proposed event, then waits until every
/// touched shoot reports `enrich_done`.
async fn import_and_enrich(base: &str, source: &Path, timeout: Duration) -> Imported {
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
                "name": if e["kind"] == "named" { json!("Enrich Test") } else { json!("") },
            })
        })
        .collect();
    let mut ws = connect_ws(base).await;
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
    let mut import_done_at = None;
    tokio::time::timeout(timeout, async {
        while let Some(msg) = ws.next().await {
            let Ok(text) = msg.unwrap().into_text() else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            match v["type"].as_str() {
                Some("import_done") if v["job_id"] == job_id.as_str() => {
                    import_done_at = Some(Instant::now());
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
    Imported {
        shoot_ids: shoot_ids.unwrap(),
        enrich_wall: import_done_at.unwrap().elapsed(),
    }
}

async fn status(base: &str) -> Value {
    reqwest::get(format!("{base}/api/status"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn open_db(root: &Path) -> photosift::db::DbPool {
    photosift::db::open(&root.join("photosift.db")).unwrap()
}

fn all_photo_ids(pool: &photosift::db::DbPool) -> Vec<i64> {
    pool.with(|c| {
        let mut stmt = c.prepare("SELECT id FROM photos ORDER BY id")?;
        let ids = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>();
        ids
    })
    .unwrap()
}

#[tokio::test]
async fn missing_models_is_not_fatal() {
    let library = tempfile::tempdir().unwrap();
    let server = spawn_server_with_library(library.path()).await;
    let st = status(&server.base_url).await;
    assert_eq!(st["ai_backend"], "unavailable", "{st}");
    assert_eq!(st["embeddings"], false, "{st}");

    let card = three_file_card();
    let imported = import_and_enrich(&server.base_url, card.path(), Duration::from_secs(60)).await;
    assert!(!imported.shoot_ids.is_empty());

    let pool = open_db(server.data_root.path());
    let ids = all_photo_ids(&pool);
    assert_eq!(ids.len(), 3);
    for id in ids {
        let p = photos::get_photo(&pool, id).unwrap().unwrap();
        assert!(p.enriched_at.is_some(), "photo {id} not marked enriched");
        assert!(p.sharpness.is_some(), "photo {id}: no sharpness");
        assert!(p.mean_luma.is_some() && p.clip_hi.is_some() && p.clip_lo.is_some());
        assert!(p.face_sharpness.is_none(), "no faces, no face sharpness");
        assert!(photos::list_faces(&pool, id).unwrap().is_empty());
    }
    // The queue drains once the worker is done.
    for _ in 0..40 {
        if status(&server.base_url).await["queue"] == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("queue never drained: {}", status(&server.base_url).await);
}

#[tokio::test]
async fn missing_preview_is_regenerated_before_analysis() {
    let library = tempfile::tempdir().unwrap();
    let server = spawn_server_with_library(library.path()).await;
    let card = three_file_card();
    import_and_enrich(&server.base_url, card.path(), Duration::from_secs(60)).await;

    // Simulate the crash window: preview gone, enrichment never recorded.
    let pool = open_db(server.data_root.path());
    let id = all_photo_ids(&pool)[0];
    let preview = server
        .data_root
        .path()
        .join("cache")
        .join("preview")
        .join(format!("{id}.jpg"));
    std::fs::remove_file(&preview).unwrap();
    pool.with(|c| {
        c.execute(
            "UPDATE photos SET enriched_at = NULL, sharpness = NULL WHERE id = ?1",
            [id],
        )
    })
    .unwrap();
    drop(pool);

    // Restarting the app resumes unenriched photos.
    let server2 = common::respawn_server(server).await;
    let pool = open_db(server2.data_root.path());
    for _ in 0..100 {
        let p = photos::get_photo(&pool, id).unwrap().unwrap();
        if p.enriched_at.is_some() {
            assert!(p.sharpness.is_some(), "sharpness from regenerated preview");
            assert!(preview.exists(), "preview regenerated");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("photo was never re-enriched");
}

#[tokio::test]
async fn attach_only_import_still_enqueues_the_touched_shoot() {
    let library = tempfile::tempdir().unwrap();
    let server = spawn_server_with_library(library.path()).await;
    let big = |pattern| {
        common::synthetic_jpeg_with_exif(
            1600,
            1200,
            pattern,
            &common::ExifSpec {
                make: Some("FUJIFILM"),
                model: Some("X100VI"),
                taken: Some("2026:10:18 10:00:00"),
                ..Default::default()
            },
        )
    };
    let raf = common::fake_raf(&big(common::Pattern::Blocks));

    let first = tempfile::tempdir().unwrap();
    write(&first.path().join("DSCF0001.RAF"), &raf);
    import_and_enrich(&server.base_url, first.path(), Duration::from_secs(60)).await;

    // Pretend the first enrichment never finished.
    let pool = open_db(server.data_root.path());
    pool.with(|c| c.execute("UPDATE photos SET enriched_at = NULL", []))
        .unwrap();

    // The same RAW comes back with its camera JPEG: nothing new, one attachment.
    let second = tempfile::tempdir().unwrap();
    write(&second.path().join("DSCF0001.RAF"), &raf);
    write(
        &second.path().join("DSCF0001.JPG"),
        &big(common::Pattern::Stripes),
    );
    let mut ws = connect_ws(&server.base_url).await;
    let http = reqwest::Client::new();
    let scan: Value = http
        .post(format!("{}/api/scan", server.base_url))
        .json(&json!({ "source": second.path() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let idxs: Vec<usize> = (0..scan["items"].as_array().unwrap().len()).collect();
    let resp = http
        .post(format!("{}/api/import", server.base_url))
        .json(&json!({
            "scan_id": scan["scan_id"],
            "events": [{ "item_idxs": idxs, "kind": "named", "name": "Enrich Test" }],
            "camera_offsets": {},
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(msg) = ws.next().await {
            let Ok(text) = msg.unwrap().into_text() else {
                continue;
            };
            let v: Value = serde_json::from_str(&text).unwrap_or_default();
            if v["type"] == "enrich_done" {
                return;
            }
        }
        panic!("websocket closed early");
    })
    .await
    .expect("attach-only import never queued enrichment");
    let id = all_photo_ids(&pool)[0];
    assert!(photos::get_photo(&pool, id)
        .unwrap()
        .unwrap()
        .enriched_at
        .is_some());
}

#[tokio::test]
async fn status_reports_unavailable_without_models() {
    let server = spawn_server().await;
    let st = status(&server.base_url).await;
    assert_eq!(st["ai_backend"], "unavailable");
    assert_eq!(st["embeddings"], false);
    assert_eq!(st["queue"], 0);
}

// ---------------------------------------------------------------- golden

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

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}

#[tokio::test]
#[ignore = "needs PHOTOSIFT_GOLDEN_DIR and the face models (scripts/fetch-models.ps1)"]
async fn golden_faces_found() {
    let dir = golden_dir();
    let models = models_dir();
    let library = tempfile::tempdir().unwrap();
    let server = spawn_server_with_models(library.path(), &models).await;
    let st = status(&server.base_url).await;
    assert_ne!(
        st["ai_backend"], "unavailable",
        "models failed to load: {st}"
    );
    assert_eq!(st["embeddings"], true, "identity model missing: {st}");

    let imported = import_and_enrich(&server.base_url, &dir, Duration::from_secs(600)).await;
    println!(
        "enrichment wall time (golden card, backend {}): {:.2} s",
        st["ai_backend"],
        imported.enrich_wall.as_secs_f64()
    );

    let pool = open_db(server.data_root.path());
    let by_stem = |stem: &str| -> i64 {
        pool.with(|c| {
            c.query_row(
                "SELECT id FROM photos WHERE raw_path LIKE ?1 OR jpeg_path LIKE ?1 \
                 ORDER BY id LIMIT 1",
                [format!("%{stem}.%")],
                |r| r.get(0),
            )
        })
        .unwrap_or_else(|e| panic!("{stem} not in the golden card: {e}"))
    };
    let (a, b) = (by_stem("DSCF1616"), by_stem("DSCF1617"));
    let fa = photos::list_faces(&pool, a).unwrap();
    let fb = photos::list_faces(&pool, b).unwrap();
    println!("DSCF1616: {} faces, DSCF1617: {} faces", fa.len(), fb.len());
    assert!(fa.len() >= 3, "DSCF1616 has {} faces", fa.len());
    let best = fa
        .iter()
        .flat_map(|x| fb.iter().map(move |y| (x, y)))
        .filter_map(|(x, y)| Some(cosine(x.embedding.as_deref()?, y.embedding.as_deref()?)))
        .fold(f32::MIN, f32::max);
    println!("best DSCF1616 x DSCF1617 embedding cosine = {best:.3}");
    assert!(best >= 0.8, "same-child cosine {best}");

    // Tiled vs full-frame-only over the whole card.
    let (det, backend) = Detector::load(&models, true).expect("detector");
    println!("test detector backend: {backend:?}");
    let preview_dir = server.data_root.path().join("cache").join("preview");
    let (mut full, mut tiled) = (0usize, 0usize);
    let mut ids: Vec<i64> = Vec::new();
    for e in std::fs::read_dir(&preview_dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "jpg") {
            let bytes = std::fs::read(&p).unwrap();
            let img = render::decode_jpeg(&bytes).unwrap();
            full += det.detect_full_frame(&img).len();
            tiled += det.detect(&img).len();
            ids.push(p.file_stem().unwrap().to_str().unwrap().parse().unwrap());
        }
    }
    let stored: usize = ids
        .iter()
        .map(|id| photos::list_faces(&pool, *id).unwrap().len())
        .sum();
    println!(
        "faces over {} photos: full-frame only = {full}, tiled = {tiled}, stored by the worker = {stored}",
        ids.len()
    );
    assert!(tiled >= full, "tiling lost faces: {tiled} < {full}");
    assert_eq!(stored, tiled, "worker output must match Detector::detect");
}
