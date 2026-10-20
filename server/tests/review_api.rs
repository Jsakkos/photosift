mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{
    spawn_server, spawn_server_with_library, spawn_server_with_web, synthetic_jpeg_with_exif,
    ExifSpec, Pattern, TestServer,
};
use photosift::db::{
    open,
    photos::{self, NewPhoto, NewShoot, ShootKind},
    DbPool,
};
use reqwest::header::HOST;
use serde_json::{json, Value};
use tempfile::TempDir;

/// A server plus a temp "library" folder holding real (tiny) files for the
/// photos seeded straight into the database. Only files created here are ever
/// sent to the Recycle Bin.
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

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.server.base_url)
    }

    fn sql(&self, sql: &str) {
        self.pool.with(|c| c.execute(sql, [])).unwrap();
    }

    /// Writes `<stem>.NEF`, `<stem>.JPG` and `<stem>.xmp` and registers the
    /// photo plus its RAW and JPEG in `files`.
    fn add_photo(&mut self, stem: &str, minute: u32) -> i64 {
        self.next += 1;
        let dir = self.folder.path();
        std::fs::write(dir.join(format!("{stem}.NEF")), b"raw bytes").unwrap();
        std::fs::write(dir.join(format!("{stem}.JPG")), b"jpeg bytes").unwrap();
        std::fs::write(dir.join(format!("{stem}.xmp")), b"<x:xmpmeta/>").unwrap();
        let raw = dir
            .join(format!("{stem}.NEF"))
            .to_string_lossy()
            .into_owned();
        let jpg = dir
            .join(format!("{stem}.JPG"))
            .to_string_lossy()
            .into_owned();
        let t = format!("2026-10-18T10:{minute:02}:00").parse().ok();
        let id = photos::insert_photo(
            &self.pool,
            &NewPhoto {
                shoot_id: self.shoot,
                raw_path: Some(raw.clone()),
                jpeg_path: Some(jpg.clone()),
                sha256: format!("hash-{}", self.next),
                camera: Some("X100VI".into()),
                capture_time: t,
                width: Some(600),
                height: Some(400),
                ..Default::default()
            },
        )
        .unwrap();
        for (path, n) in [(raw, "raw"), (jpg, "jpg")] {
            photos::insert_file(
                &self.pool,
                &format!("file-{n}-{}", self.next),
                &path,
                stem,
                9,
                Some(id),
            )
            .unwrap();
        }
        id
    }

    fn add_video_only(&mut self, minute: u32) -> i64 {
        self.next += 1;
        let path = self.folder.path().join(format!("MOV{}.MOV", self.next));
        std::fs::write(&path, b"video").unwrap();
        photos::insert_photo(
            &self.pool,
            &NewPhoto {
                shoot_id: self.shoot,
                video_path: Some(path.to_string_lossy().into_owned()),
                sha256: format!("hash-{}", self.next),
                capture_time: format!("2026-10-18T10:{minute:02}:00").parse().ok(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    /// What enrichment would have written for a photo.
    fn group(&self, id: i64, moment: i64, burst: i64, score: f64) {
        self.pool
            .with(|c| {
                c.execute(
                    "UPDATE photos SET moment_idx = ?1, burst_idx = ?2, signal_score = ?3, \
                     enriched_at = '2026-10-18T12:00:00' WHERE id = ?4",
                    rusqlite::params![moment, burst, score, id],
                )
            })
            .unwrap();
    }

    fn set_outcome(&self, id: i64, outcome: &str) {
        self.pool
            .with(|c| {
                c.execute(
                    "UPDATE photos SET outcome = ?1 WHERE id = ?2",
                    rusqlite::params![outcome, id],
                )
            })
            .unwrap();
    }

    async fn get_json(&self, path: &str) -> Value {
        let resp = self.http.get(self.url(path)).send().await.unwrap();
        assert_eq!(resp.status(), 200, "{path}");
        resp.json().await.unwrap()
    }

    async fn review(&self) -> Value {
        self.get_json(&format!("/api/shoots/{}/review", self.shoot))
            .await
    }

    async fn trash(&self) -> Value {
        let resp = self
            .http
            .post(self.url(&format!("/api/shoots/{}/trash", self.shoot)))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        resp.json().await.unwrap()
    }

    fn path(&self, name: &str) -> PathBuf {
        self.folder.path().join(name)
    }
}

fn ids_of(v: &Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap())
        .collect()
}

/// Every photo id in `moments`, in order.
fn covered(review: &Value) -> Vec<i64> {
    review["moments"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["bursts"].as_array().unwrap())
        .flat_map(|b| ids_of(&b["photo_ids"]))
        .collect()
}

// --- review -----------------------------------------------------------------

#[tokio::test]
async fn review_shape_and_order() {
    let mut env = Env::new().await;
    // Moment 0: a 3-frame burst (scores out of order) and a single.
    let (a, b, c, d) = (
        env.add_photo("A", 0),
        env.add_photo("B", 0),
        env.add_photo("C", 1),
        env.add_photo("D", 5),
    );
    env.group(a, 0, 0, 0.1);
    env.group(b, 0, 0, 0.9);
    env.group(c, 0, 0, 0.5);
    env.group(d, 0, 1, 0.3);
    // Moment 1: nine single-frame bursts, so there are 11 bursts > 8 highlights.
    let mut singles = Vec::new();
    for n in 0..9 {
        let id = env.add_photo(&format!("S{n}"), 30 + n);
        env.group(id, 1, 2 + i64::from(n), 0.04 * f64::from(n));
        singles.push(id);
    }
    // Not yet enriched, a video-only row, and one already in the Recycle Bin.
    let fresh = env.add_photo("FRESH", 59);
    let video = env.add_video_only(2);
    let gone = env.add_photo("GONE", 3);
    env.sql(&format!(
        "UPDATE photos SET trashed_at = '2026-10-18T13:00:00' WHERE id = {gone}"
    ));

    let review = env.review().await;

    let mut expected: Vec<i64> = [a, b, c, d, fresh, video]
        .into_iter()
        .chain(singles.iter().copied())
        .collect();
    let mut got = covered(&review);
    got.sort_unstable();
    expected.sort_unstable();
    assert_eq!(got, expected, "every live photo exactly once, none trashed");
    let in_map: BTreeSet<i64> = review["photos"]
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.parse().unwrap())
        .collect();
    assert_eq!(in_map, expected.iter().copied().collect());

    // Burst photo_ids by signal_score descending.
    let first = &review["moments"][0]["bursts"][0];
    assert_eq!(ids_of(&first["photo_ids"]), [b, c, a]);
    assert_eq!(first["idx"], 0);

    // Best frame of each of the 8 best bursts, best burst first.
    let highlights = ids_of(&review["highlights"]);
    assert_eq!(highlights.len(), 8);
    assert_eq!(highlights[0], b);
    assert!(!highlights.contains(&fresh) && !highlights.contains(&video));
    // Top bursts by best score: b .9, s8 .32, d .30, s7 .28 ... s3 .12.
    let want: Vec<i64> = [b, singles[8], d, singles[7], singles[6], singles[5]]
        .into_iter()
        .chain([singles[4], singles[3]])
        .collect();
    assert_eq!(highlights, want);

    let photo = &review["photos"][b.to_string()];
    assert_eq!(photo["w"], 600);
    assert_eq!(photo["h"], 400);
    assert_eq!(photo["outcome"], "unreviewed");
    assert_eq!(photo["blooper"], false);
    assert_eq!(photo["face_count"], 0);
    assert_eq!(photo["camera"], "X100VI");
    assert_eq!(photo["has_video"], false);
    assert_eq!(photo["capture_time"], "2026-10-18T10:00:00");
    assert_eq!(review["photos"][video.to_string()]["has_video"], true);
    assert!(review["photos"][fresh.to_string()]["signal_score"].is_null());

    // Counts leave the trashed photo out; the shoot is not fully enriched.
    assert_eq!(review["counts"]["total"], 15);
    assert_eq!(review["counts"]["unreviewed"], 15);
    assert_eq!(review["shoot"]["id"], env.shoot);
    assert_eq!(review["shoot"]["enriched"], false);
    assert_eq!(review["shoot"]["name"], "Test Day");
}

#[tokio::test]
async fn ungrouped_photos_land_in_the_timeline() {
    let mut env = Env::new().await;
    let (a, b) = (env.add_photo("A", 0), env.add_photo("B", 1));
    env.group(a, 0, 0, 0.5);
    env.group(b, 0, 0, 0.4);
    let video_inside = env.add_video_only(0);
    let late = env.add_photo("LATE", 50);
    // A photo with no capture time at all.
    let undated = env.add_photo("UNDATED", 0);
    env.sql(&format!(
        "UPDATE photos SET capture_time = NULL WHERE id = {undated}"
    ));

    let review = env.review().await;
    let moments = review["moments"].as_array().unwrap();

    // The video falls inside moment 0's time range, so it joins that moment.
    let m0 = &moments[0];
    assert_eq!(m0["idx"], 0);
    let m0_ids: Vec<i64> = m0["bursts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|b| ids_of(&b["photo_ids"]))
        .collect();
    assert!(m0_ids.contains(&video_inside), "{m0_ids:?}");

    // Outside every moment: one trailing moment, ordered by capture time,
    // undated last.
    let tail = moments.last().unwrap();
    assert_eq!(tail["idx"], 1);
    let tail_ids: Vec<i64> = tail["bursts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|b| ids_of(&b["photo_ids"]))
        .collect();
    assert_eq!(tail_ids, [late, undated]);

    let mut all = covered(&review);
    all.sort_unstable();
    let mut want = vec![a, b, video_inside, late, undated];
    want.sort_unstable();
    assert_eq!(all, want);
    // Burst indexes stay unique across the response.
    let bursts: Vec<i64> = moments
        .iter()
        .flat_map(|m| m["bursts"].as_array().unwrap())
        .map(|b| b["idx"].as_i64().unwrap())
        .collect();
    assert_eq!(
        bursts.iter().collect::<BTreeSet<_>>().len(),
        bursts.len(),
        "{bursts:?}"
    );
}

#[tokio::test]
async fn shoot_list_counts_and_order() {
    let mut env = Env::new().await;
    let a = env.add_photo("A", 0);
    let b = env.add_photo("B", 1);
    let c = env.add_photo("C", 2);
    env.add_photo("D", 3);
    env.set_outcome(a, "album");
    env.set_outcome(b, "delete");
    env.set_outcome(c, "delete");
    env.sql(&format!(
        "UPDATE photos SET trashed_at = '2026-10-18T13:00:00' WHERE id = {c}"
    ));
    // An older shoot sorts after the newer one.
    let older = photos::insert_shoot(
        &env.pool,
        &NewShoot {
            kind: ShootKind::Everyday,
            name: None,
            folder: "elsewhere".into(),
            date_start: "2026-09-01".parse().ok(),
            date_end: "2026-09-30".parse().ok(),
        },
    )
    .unwrap();

    let list = env.get_json("/api/shoots").await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], env.shoot);
    assert_eq!(list[1]["id"], older);
    assert_eq!(list[1]["kind"], "everyday");
    assert_eq!(list[1]["counts"]["total"], 0);
    assert_eq!(list[1]["enriched"], true);

    let s = &list[0];
    assert_eq!(s["name"], "Test Day");
    assert_eq!(s["date_start"], "2026-10-18");
    assert_eq!(s["folder"], env.folder.path().to_string_lossy().as_ref());
    assert_eq!(s["counts"]["total"], 3);
    assert_eq!(s["counts"]["album"], 1);
    assert_eq!(s["counts"]["delete"], 1);
    assert_eq!(s["counts"]["unreviewed"], 1);
    assert_eq!(s["counts"]["library"], 0);
    assert_eq!(s["counts"]["archive"], 0);
    assert_eq!(s["enriched"], false);
}

#[tokio::test]
async fn review_of_unknown_shoot_is_404() {
    let env = Env::new().await;
    let resp = env
        .http
        .get(env.url("/api/shoots/9999/review"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let resp = env
        .http
        .post(env.url("/api/shoots/9999/trash"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn review_after_a_real_import() {
    let library = tempfile::tempdir().unwrap();
    let server = spawn_server_with_library(library.path()).await;
    let card = tempfile::tempdir().unwrap();
    let dir = card.path().join("DCIM/100_FUJI");
    std::fs::create_dir_all(&dir).unwrap();
    for (n, t) in ["10:00:00", "10:00:05", "11:30:00"].iter().enumerate() {
        let jpeg = synthetic_jpeg_with_exif(
            64,
            48,
            Pattern::Blocks,
            &ExifSpec {
                make: Some("FUJIFILM"),
                model: Some("X100VI"),
                taken: Some(Box::leak(format!("2026:10:18 {t}").into_boxed_str())),
                ..Default::default()
            },
        );
        std::fs::write(dir.join(format!("DSCF000{}.JPG", n + 1)), jpeg).unwrap();
    }
    let http = reqwest::Client::new();
    let base = &server.base_url;
    let scan: Value = http
        .post(format!("{base}/api/scan"))
        .json(&json!({ "source": card.path() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let resp = http
        .post(format!("{base}/api/import"))
        .json(&json!({
            "scan_id": scan["scan_id"],
            "events": [{ "item_idxs": [0, 1, 2], "kind": "named", "name": "Test Day" }],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Wait for the import and for enrichment to finish.
    let mut shoot = Value::Null;
    for _ in 0..600 {
        let list: Value = http
            .get(format!("{base}/api/shoots"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if let Some(s) = list.as_array().unwrap().first() {
            if s["counts"]["total"] == 3 && s["enriched"] == true {
                shoot = s.clone();
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!shoot.is_null(), "import/enrichment never finished");

    let review: Value = http
        .get(format!("{base}/api/shoots/{}/review", shoot["id"]))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut got = covered(&review);
    got.sort_unstable();
    assert_eq!(got.len(), 3);
    assert_eq!(review["photos"].as_object().unwrap().len(), 3);
    assert!(review["highlights"].as_array().unwrap().len() <= 8);
    // 90 minutes apart: two moments.
    assert_eq!(review["moments"].as_array().unwrap().len(), 2);
}

// --- media ------------------------------------------------------------------

fn write_cache(env: &Env, kind: &str, id: i64, bytes: &[u8]) {
    let dir = env.server.data_root.path().join("cache").join(kind);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{id}.jpg")), bytes).unwrap();
}

#[tokio::test]
async fn preview_is_cacheable() {
    let mut env = Env::new().await;
    let id = env.add_photo("A", 0);
    write_cache(&env, "preview", id, b"preview-bytes");
    write_cache(&env, "thumb", id, b"thumb-bytes");

    for (kind, bytes) in [("preview", "preview-bytes"), ("thumb", "thumb-bytes")] {
        let resp = env
            .http
            .get(env.url(&format!("/media/{kind}/{id}")))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "{kind}");
        let cc = resp.headers()["cache-control"]
            .to_str()
            .unwrap()
            .to_string();
        assert!(cc.contains("immutable"), "{cc}");
        assert!(cc.contains("max-age=31536000"), "{cc}");
        assert_eq!(resp.headers()["content-type"], "image/jpeg");
        assert_eq!(resp.text().await.unwrap(), bytes);
    }
}

#[tokio::test]
async fn missing_media_is_404_not_the_spa() {
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(dist.path().join("index.html"), "<html>PhotoSift</html>").unwrap();
    let server = spawn_server_with_web(Some(dist.path().to_path_buf())).await;
    let http = reqwest::Client::new();
    for path in ["/media/preview/42", "/media/thumb/42"] {
        let resp = http
            .get(format!("{}{path}", server.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404, "{path}");
        assert!(!resp.text().await.unwrap().contains("PhotoSift"));
    }
}

#[tokio::test]
async fn media_obeys_the_host_guard() {
    let mut env = Env::new().await;
    let id = env.add_photo("A", 0);
    write_cache(&env, "preview", id, b"secret");
    let port = env.server.base_url.rsplit(':').next().unwrap().to_string();
    let resp = env
        .http
        .get(env.url(&format!("/media/preview/{id}")))
        .header(HOST, format!("evil.example:{port}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// --- trash ------------------------------------------------------------------

fn exists(p: &Path) -> bool {
    p.exists()
}

#[tokio::test]
async fn trash_only_approved_deletes() {
    let mut env = Env::new().await;
    let keep1 = env.add_photo("KEEP1", 0);
    let doomed = env.add_photo("DOOMED", 1);
    let keep2 = env.add_photo("KEEP2", 2);
    env.set_outcome(keep1, "album");
    env.set_outcome(doomed, "delete");

    let body = env.trash().await;
    assert_eq!(body["trashed"], 1);
    assert_eq!(body["failed"].as_array().unwrap().len(), 0);

    for ext in ["NEF", "JPG", "xmp"] {
        assert!(!exists(&env.path(&format!("DOOMED.{ext}"))), "DOOMED.{ext}");
        assert!(exists(&env.path(&format!("KEEP1.{ext}"))));
        assert!(exists(&env.path(&format!("KEEP2.{ext}"))));
    }

    let list = env.get_json("/api/shoots").await;
    assert_eq!(list[0]["counts"]["total"], 2);
    assert_eq!(list[0]["counts"]["delete"], 0);
    assert_eq!(list[0]["counts"]["album"], 1);

    let review = env.review().await;
    let mut got = covered(&review);
    got.sort_unstable();
    assert_eq!(got, [keep1, keep2]);
    assert!(review["photos"].get(doomed.to_string()).is_none());
    assert_eq!(review["counts"]["total"], 2);

    // The row and its dedupe records stay: re-importing the same junk from the
    // card must still be skipped.
    let row = photos::get_photo(&env.pool, doomed).unwrap().unwrap();
    assert!(row.trashed_at.is_some());
    let files: i64 = env
        .pool
        .with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM files WHERE photo_id = ?1",
                [doomed],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(files, 2);

    // Nothing left to approve: a second call is a no-op.
    let again = env.trash().await;
    assert_eq!(again["trashed"], 0);
}

#[tokio::test]
async fn trash_takes_the_video_too() {
    let mut env = Env::new().await;
    let photo = env.add_photo("P", 0);
    let video = env.add_video_only(1);
    env.set_outcome(photo, "delete");
    env.set_outcome(video, "delete");
    let video_path = photos::get_photo(&env.pool, video)
        .unwrap()
        .unwrap()
        .video_path
        .unwrap();

    let body = env.trash().await;
    assert_eq!(body["trashed"], 2);
    assert!(!exists(Path::new(&video_path)));
    assert!(!exists(&env.path("P.NEF")));
}

#[tokio::test]
async fn trash_tolerates_files_already_missing() {
    let mut env = Env::new().await;
    let id = env.add_photo("A", 0);
    env.set_outcome(id, "delete");
    std::fs::remove_file(env.path("A.JPG")).unwrap();
    std::fs::remove_file(env.path("A.xmp")).unwrap();

    let body = env.trash().await;
    assert_eq!(body["trashed"], 1);
    assert!(!exists(&env.path("A.NEF")));
}

#[cfg(windows)]
#[tokio::test]
async fn trash_failure_reported_not_fatal() {
    use std::os::windows::fs::OpenOptionsExt;

    let mut env = Env::new().await;
    let ids: Vec<i64> = ["A", "B", "C"]
        .iter()
        .enumerate()
        .map(|(n, s)| env.add_photo(s, n as u32))
        .collect();
    for id in &ids {
        env.set_outcome(*id, "delete");
    }

    // share_mode(0) denies every other open, including the shell's move.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(env.path("B.NEF"))
        .unwrap();

    let body = env.trash().await;
    assert_eq!(body["trashed"], 2, "{body}");
    let failed = body["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{body}");
    assert_eq!(failed[0]["photo_id"], ids[1]);
    assert!(!failed[0]["error"].as_str().unwrap().is_empty());

    assert!(exists(&env.path("B.NEF")), "locked file must stay");
    for stem in ["A", "C"] {
        for ext in ["NEF", "JPG", "xmp"] {
            assert!(!exists(&env.path(&format!("{stem}.{ext}"))), "{stem}.{ext}");
        }
    }
    let b = photos::get_photo(&env.pool, ids[1]).unwrap().unwrap();
    assert!(b.trashed_at.is_none(), "failed photo stays in the review");
    assert_eq!(env.get_json("/api/shoots").await[0]["counts"]["total"], 1);

    // Once the lock is gone a retry finishes the job, including whatever part
    // of B was already trashed.
    drop(lock);
    let retry = env.trash().await;
    assert_eq!(retry["trashed"], 1, "{retry}");
    assert!(retry["failed"].as_array().unwrap().is_empty());
    assert!(!exists(&env.path("B.NEF")));
}

// --- trash safety -----------------------------------------------------------

#[tokio::test]
async fn trash_skips_photo_flipped_after_the_snapshot() {
    let mut env = Env::new().await;
    let flipped = env.add_photo("FLIPPED", 0);
    let doomed = env.add_photo("DOOMED", 1);
    env.set_outcome(flipped, "delete");
    env.set_outcome(doomed, "delete");
    let snapshot = photos::pending_deletes(&env.pool, env.shoot).unwrap();
    assert_eq!(snapshot.len(), 2);

    // The user changes their mind after the list was built, before it is processed.
    photos::set_outcome(&env.pool, flipped, photosift::outcome::Outcome::Album, None).unwrap();

    let report = photosift::trash::trash_photos(&env.pool, env.shoot, snapshot);
    assert_eq!(report.trashed, 1);
    assert!(report.failed.is_empty());
    for ext in ["NEF", "JPG", "xmp"] {
        assert!(
            exists(&env.path(&format!("FLIPPED.{ext}"))),
            "FLIPPED.{ext}"
        );
        assert!(!exists(&env.path(&format!("DOOMED.{ext}"))));
    }
    let row = photos::get_photo(&env.pool, flipped).unwrap().unwrap();
    assert!(row.trashed_at.is_none());
}

#[tokio::test]
async fn overlapping_batches_count_each_photo_once() {
    let mut env = Env::new().await;
    let id = env.add_photo("A", 0);
    env.set_outcome(id, "delete");
    let snapshot = photos::pending_deletes(&env.pool, env.shoot).unwrap();

    let first = photosift::trash::trash_photos(&env.pool, env.shoot, snapshot.clone());
    let second = photosift::trash::trash_photos(&env.pool, env.shoot, snapshot);
    assert_eq!(first.trashed, 1);
    assert_eq!(second.trashed, 0);
    assert!(second.failed.is_empty());
}

#[tokio::test]
async fn unavailable_library_folder_is_a_failure_not_a_trash() {
    let mut env = Env::new().await;
    let id = env.add_photo("A", 0);
    env.set_outcome(id, "delete");
    // The whole folder is unreachable, like an offline drive.
    std::fs::remove_dir_all(env.folder.path()).unwrap();

    let body = env.trash().await;
    assert_eq!(body["trashed"], 0, "{body}");
    assert_eq!(body["failed"][0]["photo_id"], id);
    assert_eq!(body["failed"][0]["error"], "library folder not available");
    let row = photos::get_photo(&env.pool, id).unwrap().unwrap();
    assert!(row.trashed_at.is_none(), "must stay in the review");
    assert_eq!(env.get_json("/api/shoots").await[0]["counts"]["delete"], 1);

    // Back online with the files already gone: nothing left to lose, so finish.
    std::fs::create_dir_all(env.folder.path()).unwrap();
    let body = env.trash().await;
    assert_eq!(body["trashed"], 1, "{body}");
}
