mod common;

use base64::Engine;
use common::{spawn_server, synthetic_jpeg_plain, TestServer};
use photosift::db::{
    open,
    photos::{self, FaceRow, NewFace, NewPhoto, NewShoot, ShootKind},
    DbPool,
};
use photosift::media::{crops, render::Rgb};
use reqwest::header::{ACCEPT, CONTENT_TYPE, ORIGIN};
use serde_json::{json, Value};
use tempfile::TempDir;

struct Env {
    server: TestServer,
    folder: TempDir,
    pool: DbPool,
    http: reqwest::Client,
    shoot: i64,
    next: i64,
}

impl Env {
    async fn new() -> Env {
        let server = spawn_server().await;
        let pool = open(&server.data_root.path().join("photosift.db")).unwrap();
        let folder = tempfile::tempdir().unwrap();
        let shoot = photos::insert_shoot(
            &pool,
            &NewShoot {
                kind: ShootKind::Named,
                name: Some("Test Day".into()),
                folder: folder.path().to_string_lossy().into_owned(),
                date_start: "2026-10-18".parse().ok(),
                date_end: "2026-10-18".parse().ok(),
            },
        )
        .unwrap();
        Env {
            server,
            folder,
            pool,
            http: reqwest::Client::new(),
            shoot,
            next: 0,
        }
    }

    fn url(&self) -> String {
        format!("{}/mcp", self.server.base_url)
    }

    /// A photo with a RAW stand-in and cached preview (2000x1000) and
    /// thumbnail (400x200) JPEGs.
    fn add_photo(&mut self, minute: u32, score: f64, moment: i64, burst: i64) -> i64 {
        self.next += 1;
        let stem = format!("P{}", self.next);
        let raw = self.folder.path().join(format!("{stem}.NEF"));
        std::fs::write(&raw, b"raw bytes").unwrap();
        let id = photos::insert_photo(
            &self.pool,
            &NewPhoto {
                shoot_id: self.shoot,
                raw_path: Some(raw.to_string_lossy().into_owned()),
                sha256: format!("hash-{}", self.next),
                camera: Some("X100VI".into()),
                capture_time: format!("2026-10-18T10:{minute:02}:00").parse().ok(),
                width: Some(2000),
                height: Some(1000),
                ..Default::default()
            },
        )
        .unwrap();
        self.pool
            .with(|c| {
                c.execute(
                    "UPDATE photos SET moment_idx = ?1, burst_idx = ?2, signal_score = ?3, \
                     enriched_at = '2026-10-18T12:00:00' WHERE id = ?4",
                    rusqlite::params![moment, burst, score, id],
                )
            })
            .unwrap();
        self.cache("preview", id, 2000, 1000);
        self.cache("thumb", id, 400, 200);
        id
    }

    fn cache(&self, kind: &str, id: i64, w: u32, h: u32) {
        let dir = self.server.data_root.path().join("cache").join(kind);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.jpg")), synthetic_jpeg_plain(w, h)).unwrap();
    }

    fn trash(&self, id: i64) {
        self.pool
            .with(|c| {
                c.execute(
                    "UPDATE photos SET trashed_at = '2026-10-18T13:00:00' WHERE id = ?1",
                    [id],
                )
            })
            .unwrap();
    }

    async fn post(&self, body: Value) -> reqwest::Response {
        self.http
            .post(self.url())
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .header("MCP-Protocol-Version", "2025-06-18")
            .body(body.to_string())
            .send()
            .await
            .unwrap()
    }

    async fn rpc(&self, method: &str, params: Value) -> Value {
        let resp = self
            .post(json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()[CONTENT_TYPE].to_str().unwrap(),
            "application/json"
        );
        resp.json().await.unwrap()
    }

    /// The `result` of a `tools/call`.
    async fn call(&self, name: &str, args: Value) -> Value {
        let v = self
            .rpc("tools/call", json!({"name": name, "arguments": args}))
            .await;
        assert!(v.get("error").is_none(), "{v}");
        v["result"].clone()
    }
}

fn blocks(result: &Value) -> &Vec<Value> {
    result["content"].as_array().unwrap()
}

fn text(block: &Value) -> &str {
    assert_eq!(block["type"], "text", "{block}");
    block["text"].as_str().unwrap()
}

fn image_long_edge(block: &Value) -> u32 {
    assert_eq!(block["type"], "image");
    assert_eq!(block["mimeType"], "image/jpeg");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(block["data"].as_str().unwrap())
        .unwrap();
    let img = photosift::media::render::decode_jpeg(&bytes).unwrap();
    img.w.max(img.h)
}

#[tokio::test]
async fn initialize_and_list_tools() {
    let env = Env::new().await;
    let init = env
        .rpc(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "t", "version": "0"}}),
        )
        .await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "photosift");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    let list = env.rpc("tools/list", json!({})).await;
    let tools = list["result"]["tools"].as_array().unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "get_faces",
            "get_moment",
            "get_photos",
            "get_shoot_overview",
            "get_taste_profile",
            "list_shoots",
            "set_outcome"
        ]
    );
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        assert!(!t["description"].as_str().unwrap().is_empty());
    }

    assert_eq!(env.rpc("ping", json!({})).await["result"], json!({}));
    assert_eq!(env.rpc("nope", json!({})).await["error"]["code"], -32601);
}

#[tokio::test]
async fn notification_gets_202() {
    let env = Env::new().await;
    let resp = env
        .post(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .await;
    assert_eq!(resp.status(), 202);
    assert!(resp.bytes().await.unwrap().is_empty());
}

#[tokio::test]
async fn get_is_405() {
    let env = Env::new().await;
    let resp = env.http.get(env.url()).send().await.unwrap();
    assert_eq!(resp.status(), 405);
}

#[tokio::test]
async fn get_moment_returns_labelled_thumbnails() {
    let mut env = Env::new().await;
    let scores = [0.2, 0.9, 0.5, 0.1, 0.7, 0.3];
    let ids: Vec<i64> = scores
        .iter()
        .enumerate()
        .map(|(i, s)| env.add_photo(i as u32, *s, 0, 0))
        .collect();
    let result = env
        .call(
            "get_moment",
            json!({"shoot_id": env.shoot, "moment_idx": 0, "frames_per_burst": 3}),
        )
        .await;
    assert_eq!(result["isError"], false, "{result}");
    let content = blocks(&result);
    let images: Vec<usize> = (0..content.len())
        .filter(|&i| content[i]["type"] == "image")
        .collect();
    assert_eq!(images.len(), 3);
    let want = [ids[1], ids[4], ids[2]];
    for (&at, id) in images.iter().zip(want) {
        assert!(at > 0);
        assert!(
            text(&content[at - 1]).contains(&format!("photo {id}")),
            "{}",
            content[at - 1]
        );
    }
    // Every frame of the burst gets a metadata line, not only the pictured ones.
    let all_text: String = content
        .iter()
        .filter(|b| b["type"] == "text")
        .map(|b| text(b).to_string())
        .collect();
    for id in &ids {
        assert!(all_text.contains(&format!("id={id} ")), "{all_text}");
    }
}

#[tokio::test]
async fn get_photos_caps_and_downsizes() {
    let mut env = Env::new().await;
    let ids: Vec<i64> = (0..9)
        .map(|i| env.add_photo(i, 0.5, 0, i64::from(i)))
        .collect();
    let r = env.call("get_photos", json!({"photo_ids": ids})).await;
    assert_eq!(r["isError"], true);

    let r = env
        .call("get_photos", json!({"photo_ids": &ids[..2]}))
        .await;
    assert_eq!(r["isError"], false, "{r}");
    let imgs: Vec<&Value> = blocks(&r).iter().filter(|b| b["type"] == "image").collect();
    assert_eq!(imgs.len(), 2);
    for i in imgs {
        assert_eq!(image_long_edge(i), 1024);
    }
}

#[test]
fn face_crop_square_and_clamped() {
    let (w, h) = (300u32, 200u32);
    let px = vec![128u8; (w * h * 3) as usize];
    let preview = Rgb { w, h, px };
    let face = FaceRow {
        id: 1,
        photo_id: 1,
        bbox: (0.0, 0.0, 0.1, 0.15),
        score: None,
        landmarks: None,
        embedding: None,
        sharpness: None,
    };
    let jpeg = crops::face_crop(&preview, &face, 256).unwrap();
    let out = photosift::media::render::decode_jpeg(&jpeg).unwrap();
    assert_eq!((out.w, out.h), (256, 256));
    let far = FaceRow {
        bbox: (0.95, 0.95, 0.2, 0.2),
        ..face
    };
    let jpeg = crops::face_crop(&preview, &far, 256).unwrap();
    let out = photosift::media::render::decode_jpeg(&jpeg).unwrap();
    assert_eq!((out.w, out.h), (256, 256));
}

#[tokio::test]
async fn get_faces_labels_crops() {
    let mut env = Env::new().await;
    let id = env.add_photo(0, 0.5, 0, 0);
    photos::save_faces_only(
        &env.pool,
        id,
        &[NewFace {
            bbox: (0.4, 0.3, 0.1, 0.2),
            landmarks: [(0.0, 0.0); 5],
            score: 0.9,
            embedding: None,
            sharpness: Some(42.0),
        }],
        "test",
    )
    .unwrap();
    let r = env.call("get_faces", json!({"photo_ids": [id]})).await;
    assert_eq!(r["isError"], false, "{r}");
    let c = blocks(&r);
    assert!(text(&c[0]).contains(&format!("photo {id} face 1 (sharpness 42")));
    assert_eq!(image_long_edge(&c[1]), 256);
}

#[tokio::test]
async fn trashed_or_unknown_ids_are_tool_errors() {
    let mut env = Env::new().await;
    let a = env.add_photo(0, 0.5, 0, 0);
    let gone = env.add_photo(1, 0.5, 0, 1);
    env.trash(gone);
    let r = env
        .call("get_photos", json!({"photo_ids": [a, gone]}))
        .await;
    assert_eq!(r["isError"], true);
    assert!(text(&blocks(&r)[0]).contains(&gone.to_string()));
    let r = env.call("get_photos", json!({"photo_ids": [99999]})).await;
    assert_eq!(r["isError"], true);
    assert!(text(&blocks(&r)[0]).contains("99999"));
}

#[tokio::test]
async fn foreign_origin_rejected() {
    let env = Env::new().await;
    let resp = env
        .http
        .post(env.url())
        .header(ORIGIN, "http://evil.example")
        .header(CONTENT_TYPE, "application/json")
        .body(json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn set_outcome_via_mcp_writes_xmp() {
    let mut env = Env::new().await;
    let id = env.add_photo(0, 0.5, 0, 0);
    let r = env
        .call("set_outcome", json!({"photo_id": id, "outcome": "album"}))
        .await;
    assert_eq!(r["isError"], false, "{r}");
    let xmp = std::fs::read_to_string(env.folder.path().join("P1.xmp")).unwrap();
    assert!(
        xmp.contains("xmp:Rating=\"4\"") || xmp.contains(">4<"),
        "{xmp}"
    );
    let row = photos::get_photo(&env.pool, id).unwrap().unwrap();
    assert_eq!(row.outcome.as_str(), "album");

    let bad = env
        .call("set_outcome", json!({"photo_id": id, "outcome": "bogus"}))
        .await;
    assert_eq!(bad["isError"], true);
}

#[tokio::test]
async fn overview_and_list_shoots() {
    let mut env = Env::new().await;
    env.add_photo(0, 0.5, 0, 0);
    env.add_photo(1, 0.4, 0, 0);
    let l = env.call("list_shoots", json!({})).await;
    assert!(text(&blocks(&l)[0]).contains("Test Day"));
    let o = env
        .call("get_shoot_overview", json!({"shoot_id": env.shoot}))
        .await;
    assert_eq!(o["isError"], false, "{o}");
    let t = text(&blocks(&o)[0]);
    assert!(t.contains("X100VI") && t.contains("Test Day"), "{t}");
    let e = env
        .call("get_shoot_overview", json!({"shoot_id": 12345}))
        .await;
    assert_eq!(e["isError"], true);
    let tp = env.call("get_taste_profile", json!({})).await;
    assert_eq!(tp["isError"], false);
}
