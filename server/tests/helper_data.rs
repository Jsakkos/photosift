mod common;

use common::{spawn_server, TestServer};
use photosift::db::{
    open,
    photos::{self, NewPhoto, NewShoot, ShootKind},
    DbPool,
};
use photosift::helper::learning;
use photosift::outcome::Outcome;
use reqwest::header::CONTENT_TYPE;
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
        let mut env = Env {
            server,
            folder,
            pool,
            http: reqwest::Client::new(),
            shoot: 0,
            next: 0,
        };
        env.shoot = env.add_shoot("Test Day");
        env
    }

    fn add_shoot(&self, name: &str) -> i64 {
        photos::insert_shoot(
            &self.pool,
            &NewShoot {
                kind: ShootKind::Named,
                name: Some(name.into()),
                folder: self.folder.path().join(name).to_string_lossy().into_owned(),
                date_start: "2026-10-18".parse().ok(),
                date_end: "2026-10-18".parse().ok(),
            },
        )
        .unwrap()
    }

    /// A grouped photo with a RAW stand-in, so outcomes write `P<n>.xmp`.
    fn add_photo_in(&mut self, shoot: i64) -> i64 {
        self.next += 1;
        let raw = self.folder.path().join(format!("P{}.NEF", self.next));
        std::fs::write(&raw, b"raw bytes").unwrap();
        let id = photos::insert_photo(
            &self.pool,
            &NewPhoto {
                shoot_id: shoot,
                raw_path: Some(raw.to_string_lossy().into_owned()),
                sha256: format!("hash-{}", self.next),
                camera: Some("X100VI".into()),
                capture_time: format!("2026-10-18T10:{:02}:00", self.next).parse().ok(),
                width: Some(2000),
                height: Some(1000),
                ..Default::default()
            },
        )
        .unwrap();
        self.pool
            .with(|c| {
                c.execute(
                    "UPDATE photos SET moment_idx = 0, burst_idx = ?1, signal_score = 0.5, \
                     enriched_at = '2026-10-18T12:00:00' WHERE id = ?2",
                    rusqlite::params![self.next, id],
                )
            })
            .unwrap();
        id
    }

    fn add_photo(&mut self) -> i64 {
        self.add_photo_in(self.shoot)
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

    fn count(&self, table: &str) -> i64 {
        self.pool
            .with(|c| c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)))
            .unwrap()
    }

    /// The `result` of a `tools/call`.
    async fn call(&self, name: &str, args: Value) -> Value {
        let v: Value = self
            .http
            .post(format!("{}/mcp", self.server.base_url))
            .header(CONTENT_TYPE, "application/json")
            .body(
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                       "params": {"name": name, "arguments": args}})
                .to_string(),
            )
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(v.get("error").is_none(), "{v}");
        v["result"].clone()
    }

    async fn start_run(&self, shoot: i64) -> i64 {
        let r = self.call("start_run", json!({"shoot_id": shoot})).await;
        assert_eq!(r["isError"], false, "{r}");
        let t = text(&r);
        t.split(|c: char| !c.is_ascii_digit())
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| panic!("no run id in {t}"))
            .parse()
            .unwrap()
    }

    async fn propose(&self, run: i64, items: Value) -> Value {
        self.call("propose", json!({"run_id": run, "items": items}))
            .await
    }

    async fn set_outcome(&self, id: i64, outcome: &str) {
        let resp = self
            .http
            .post(format!("{}/api/photos/{id}/outcome", self.server.base_url))
            .json(&json!({"outcome": outcome}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    async fn review(&self) -> Value {
        let resp = self
            .http
            .get(format!(
                "{}/api/shoots/{}/review",
                self.server.base_url, self.shoot
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        resp.json().await.unwrap()
    }

    fn decision(&self, photo: i64) -> Option<(String, String, Option<String>)> {
        self.pool
            .with(|c| {
                use rusqlite::OptionalExtension;
                c.query_row(
                    "SELECT proposed, chosen, criterion FROM decisions WHERE photo_id = ?1",
                    [photo],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
            })
            .unwrap()
    }
}

fn text(result: &Value) -> String {
    result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn item(photo: i64, outcome: &str, reason: &str) -> Value {
    json!({"photo_id": photo, "outcome": outcome, "reason": reason})
}

#[tokio::test]
async fn proposal_never_changes_outcome() {
    let mut env = Env::new().await;
    let p = env.add_photo();
    env.set_outcome(p, "album").await;
    let sidecar = env.folder.path().join("P1.xmp");
    let before = std::fs::metadata(&sidecar).unwrap().modified().unwrap();
    let xmp_before = std::fs::read(&sidecar).unwrap();

    let run = env.start_run(env.shoot).await;
    let r = env
        .propose(run, json!([item(p, "delete", "eyes closed")]))
        .await;
    assert_eq!(r["isError"], false, "{r}");

    let row = photos::get_photo(&env.pool, p).unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::Album);
    assert_eq!(
        std::fs::metadata(&sidecar).unwrap().modified().unwrap(),
        before
    );
    assert_eq!(std::fs::read(&sidecar).unwrap(), xmp_before);
    let review = env.review().await;
    let photo = &review["photos"][p.to_string()];
    assert_eq!(photo["outcome"], "album");
    assert_eq!(photo["proposal"]["outcome"], "delete");
    assert_eq!(photo["proposal"]["reason"], "eyes closed");
    assert_eq!(review["helper"]["suggested"]["delete"], 0);
    assert_eq!(review["helper"]["status"], "running");
    assert_eq!(review["helper"]["run_id"], run);
}

#[tokio::test]
async fn propose_rejects_foreign_trashed_and_invalid() {
    let mut env = Env::new().await;
    let good = env.add_photo();
    let gone = env.add_photo();
    env.trash(gone);
    let other_shoot = env.add_shoot("Other Day");
    let foreign = env.add_photo_in(other_shoot);
    let run = env.start_run(env.shoot).await;

    let long = "x".repeat(300);
    let cases = [
        (item(foreign, "album", "nice"), foreign.to_string()),
        (item(gone, "album", "nice"), gone.to_string()),
        (item(good, "pick", "nice"), "pick".to_string()),
        (item(good, "album", &long), "reason".to_string()),
        (item(good, "album", "   "), "reason".to_string()),
    ];
    for (bad, needle) in cases {
        let r = env
            .propose(run, json!([item(good, "library", "fine"), bad]))
            .await;
        assert_eq!(r["isError"], true, "{r}");
        let t = text(&r);
        assert!(t.contains("item 1"), "{t}");
        assert!(t.contains(&needle), "{needle} not in {t}");
    }
    // Every bad item is listed, not just the first.
    let r = env
        .propose(
            run,
            json!([item(foreign, "album", "a"), item(good, "pick", "b")]),
        )
        .await;
    let t = text(&r);
    assert!(t.contains("item 0") && t.contains("item 1"), "{t}");
    assert_eq!(env.count("proposals"), 0);
}

#[tokio::test]
async fn writes_to_finished_run_rejected() {
    let mut env = Env::new().await;
    let p = env.add_photo();
    let run = env.start_run(env.shoot).await;
    let r = env
        .call(
            "finish_run",
            json!({"run_id": run, "summary": "A short party."}),
        )
        .await;
    assert_eq!(r["isError"], false, "{r}");
    let review = env.review().await;
    assert_eq!(review["helper"]["status"], "done");
    assert_eq!(review["helper"]["summary"], "A short party.");

    let r = env.propose(run, json!([item(p, "album", "smiles")])).await;
    assert_eq!(r["isError"], true);
    assert!(
        text(&r).contains(&format!("run {run} is not running")),
        "{r}"
    );
    let r = env.propose(9999, json!([item(p, "album", "smiles")])).await;
    assert_eq!(r["isError"], true);
    assert_eq!(env.count("proposals"), 0);

    let r = env
        .call(
            "finish_run",
            json!({"run_id": env.start_run(env.shoot).await, "summary": "y".repeat(601)}),
        )
        .await;
    assert_eq!(r["isError"], true);
}

#[tokio::test]
async fn newer_run_overwrites_proposal() {
    let mut env = Env::new().await;
    let p = env.add_photo();
    let run1 = env.start_run(env.shoot).await;
    env.propose(run1, json!([item(p, "album", "first look")]))
        .await;
    let run2 = env.start_run(env.shoot).await;
    let r = env
        .propose(
            run2,
            json!([{"photo_id": p, "outcome": "library", "reason": "second look", "blooper": true}]),
        )
        .await;
    assert_eq!(r["isError"], false, "{r}");
    let review = env.review().await;
    let prop = &review["photos"][p.to_string()]["proposal"];
    assert_eq!(prop["outcome"], "library");
    assert_eq!(prop["reason"], "second look");
    assert_eq!(prop["blooper"], true);
    assert!(prop["criterion"].is_null());
    assert_eq!(review["helper"]["run_id"], run2);
    assert_eq!(review["helper"]["suggested"]["library"], 1);
    assert_eq!(review["helper"]["suggested"]["total"], 1);
    assert_eq!(env.count("proposals"), 1);
}

#[tokio::test]
async fn decision_recorded_for_accept_and_override() {
    let mut env = Env::new().await;
    let p = env.add_photo();
    let run = env.start_run(env.shoot).await;
    env.propose(run, json!([item(p, "album", "big laugh")]))
        .await;

    env.set_outcome(p, "album").await;
    assert_eq!(
        env.decision(p),
        Some(("album".into(), "album".into(), None))
    );
    let ex = learning::recent_examples(&env.pool, 10, 10).unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(ex[0].chosen, ex[0].proposed);

    env.set_outcome(p, "library").await;
    assert_eq!(
        env.decision(p),
        Some(("album".into(), "library".into(), None))
    );
    assert_eq!(env.count("decisions"), 1);
    let ex = learning::recent_examples(&env.pool, 10, 10).unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(ex[0].proposed, Outcome::Album);
    assert_eq!(ex[0].proposed_reason, "big laugh");
    assert_eq!(ex[0].chosen, Outcome::Library);
    // An override is not counted as an accept.
    assert!(learning::recent_examples(&env.pool, 0, 10)
        .unwrap()
        .is_empty());

    env.set_outcome(p, "unreviewed").await;
    assert_eq!(env.decision(p), None);

    // MCP set_outcome records too.
    let r = env
        .call("set_outcome", json!({"photo_id": p, "outcome": "archive"}))
        .await;
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(
        env.decision(p),
        Some(("album".into(), "archive".into(), None))
    );
}

#[tokio::test]
async fn decision_note_sets_criterion() {
    let mut env = Env::new().await;
    let p = env.add_photo();
    let url = format!("{}/api/photos/{p}/decision-note", env.server.base_url);
    let note = json!({"criterion": "expression", "note": "her smile"});
    let resp = env.http.post(&url).json(&note).send().await.unwrap();
    assert_eq!(resp.status(), 404);

    let run = env.start_run(env.shoot).await;
    env.propose(run, json!([item(p, "archive", "soft focus")]))
        .await;
    env.set_outcome(p, "album").await;
    let resp = env.http.post(&url).json(&note).send().await.unwrap();
    assert_eq!(resp.status(), 204);
    assert_eq!(
        env.decision(p),
        Some(("archive".into(), "album".into(), Some("expression".into())))
    );
    let ex = learning::recent_examples(&env.pool, 10, 10).unwrap();
    assert_eq!(ex[0].criterion.as_deref(), Some("expression"));
    assert_eq!(ex[0].note.as_deref(), Some("her smile"));
    let review = env.review().await;
    assert_eq!(
        review["photos"][p.to_string()]["proposal"]["criterion"],
        "expression"
    );

    let resp = env
        .http
        .post(&url)
        .json(&json!({"criterion": "vibes"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn your_call_pair_round_trip() {
    let mut env = Env::new().await;
    let a = env.add_photo();
    let b = env.add_photo();
    let run = env.start_run(env.shoot).await;
    let r = env
        .call(
            "propose_your_call",
            json!({"run_id": run, "photo_a": a, "photo_b": b,
                   "tradeoff": "A is sharper; B has the better smile"}),
        )
        .await;
    assert_eq!(r["isError"], false, "{r}");
    let review = env.review().await;
    let calls = review["helper"]["your_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["a"], a);
    assert_eq!(calls[0]["b"], b);
    assert_eq!(calls[0]["tradeoff"], "A is sharper; B has the better smile");
    assert!(calls[0]["id"].is_i64());

    let r = env
        .call(
            "propose_your_call",
            json!({"run_id": run, "photo_a": b, "photo_b": a, "tradeoff": "again"}),
        )
        .await;
    assert_eq!(r["isError"], true, "{r}");
    let r = env
        .call(
            "propose_your_call",
            json!({"run_id": run, "photo_a": a, "photo_b": a, "tradeoff": "same"}),
        )
        .await;
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(env.count("your_call_pairs"), 1);
}

#[tokio::test]
async fn taste_round_trip_and_cap() {
    let env = Env::new().await;
    let r = env
        .call("set_taste_profile", json!({"text": "x".repeat(4001)}))
        .await;
    assert_eq!(r["isError"], true, "{r}");
    let r = env
        .call("set_taste_profile", json!({"text": "prefers candid"}))
        .await;
    assert_eq!(r["isError"], false, "{r}");
    let r = env.call("get_taste_profile", json!({})).await;
    assert_eq!(r["isError"], false);
    assert!(text(&r).contains("prefers candid"), "{r}");
}

#[tokio::test]
async fn moment_and_overview_show_proposals() {
    let mut env = Env::new().await;
    let a = env.add_photo();
    let b = env.add_photo();
    let run = env.start_run(env.shoot).await;
    env.propose(
        run,
        json!([{"photo_id": a, "outcome": "library", "reason": "funny face", "blooper": true}]),
    )
    .await;
    let r = env
        .call(
            "get_moment",
            json!({"shoot_id": env.shoot, "moment_idx": 0}),
        )
        .await;
    let t = text(&r);
    assert!(t.contains(&format!("id={a} ")), "{t}");
    assert!(t.contains("proposal=library+blooper"), "{t}");
    let line_b = t
        .lines()
        .find(|l| l.starts_with(&format!("id={b} ")))
        .unwrap();
    assert!(line_b.ends_with("proposal=-"), "{line_b}");

    let r = env
        .call("get_shoot_overview", json!({"shoot_id": env.shoot}))
        .await;
    let t = text(&r);
    assert!(
        t.contains("Proposals for unreviewed photos: album 0, library 1"),
        "{t}"
    );
    assert!(t.contains("(1 of 2 unreviewed)"), "{t}");
}

#[tokio::test]
async fn writes_recheck_run_status_in_the_transaction() {
    use photosift::db::helper::{self, NewProposal, RunStatus, YourCallError};
    let mut env = Env::new().await;
    let a = env.add_photo();
    let b = env.add_photo();
    let run = env.start_run(env.shoot).await;
    // The runner failed the run after a tool's first check passed.
    helper::end_open_run(&env.pool, run, RunStatus::Failed, Some("usage limit")).unwrap();

    let proposal = NewProposal {
        photo_id: a,
        outcome: Outcome::Album,
        blooper: false,
        reason: "late".into(),
    };
    let e = helper::upsert_proposals(&env.pool, run, &[proposal]).unwrap_err();
    let nr = e.downcast_ref::<helper::NotRunning>().expect("NotRunning");
    assert_eq!(nr.status, "failed");
    assert!(matches!(
        helper::add_your_call(&env.pool, run, a, b, "coin flip"),
        Err(YourCallError::NotRunning(_))
    ));
    let e = helper::complete_run(&env.pool, run, "late summary").unwrap_err();
    assert!(e.downcast_ref::<helper::NotRunning>().is_some(), "{e:#}");
    let row = helper::get_run(&env.pool, run).unwrap().unwrap();
    assert_eq!(row.status, "failed");
    assert_eq!(row.summary, None);
    assert_eq!(env.count("proposals"), 0);
    assert_eq!(env.count("your_call_pairs"), 0);
}

#[tokio::test]
async fn decision_note_targets_current_proposal() {
    let mut env = Env::new().await;
    let p = env.add_photo();
    let url = format!("{}/api/photos/{p}/decision-note", env.server.base_url);
    let run1 = env.start_run(env.shoot).await;
    env.propose(run1, json!([item(p, "archive", "soft focus")]))
        .await;
    env.set_outcome(p, "album").await;
    // A newer run proposes again; the user has not decided on it yet.
    let run2 = env.start_run(env.shoot).await;
    env.propose(run2, json!([item(p, "library", "second look")]))
        .await;
    let note = json!({"criterion": "expression"});
    let resp = env.http.post(&url).json(&note).send().await.unwrap();
    assert_eq!(resp.status(), 404);
    assert_eq!(
        env.decision(p),
        Some(("archive".into(), "album".into(), None)),
        "the old run's decision is untouched"
    );
}

impl Env {
    async fn taste(&self) -> Value {
        let resp = self
            .http
            .get(format!("{}/api/taste", self.server.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        resp.json().await.unwrap()
    }

    async fn put_taste(&self, text: &str) -> reqwest::Response {
        self.http
            .put(format!("{}/api/taste", self.server.base_url))
            .json(&json!({ "text": text }))
            .send()
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn taste_http_round_trip() {
    let env = Env::new().await;
    let empty = env.taste().await;
    assert_eq!(empty["text"], "");

    let resp = env.put_taste("prefers candid over posed").await;
    assert_eq!(resp.status(), 200);
    let put: Value = resp.json().await.unwrap();
    assert_eq!(put["text"], "prefers candid over posed");
    assert!(!put["updated_at"].as_str().unwrap().is_empty());

    assert_eq!(env.taste().await["text"], "prefers candid over posed");
    assert_eq!(env.put_taste(&"x".repeat(4000)).await.status(), 200);
    let resp = env.put_taste(&"x".repeat(4001)).await;
    assert_eq!(resp.status(), 400);
    // The rejected save left the stored profile alone.
    assert_eq!(env.taste().await["text"], "x".repeat(4000));
}

#[tokio::test]
async fn helper_taste_write_marked() {
    let env = Env::new().await;
    assert_eq!(env.taste().await["updated_by"], "user");

    let r = env
        .call("set_taste_profile", json!({"text": "likes dusk"}))
        .await;
    assert_eq!(r["isError"], false, "{r}");
    let t = env.taste().await;
    assert_eq!(t["text"], "likes dusk");
    assert_eq!(t["updated_by"], "helper");

    env.put_taste("likes dusk and dawn").await;
    assert_eq!(env.taste().await["updated_by"], "user");
}

#[tokio::test]
async fn decisions_listing_newest_first() {
    let mut env = Env::new().await;
    let a = env.add_photo();
    let b = env.add_photo();
    let c = env.add_photo();
    let run = env.start_run(env.shoot).await;
    env.propose(
        run,
        json!([
            item(a, "album", "big laugh"),
            item(b, "album", "sharp"),
            item(c, "album", "kept"),
        ]),
    )
    .await;
    env.set_outcome(a, "library").await;
    env.set_outcome(b, "archive").await;
    // An accept is not a disagreement.
    env.set_outcome(c, "album").await;
    env.http
        .post(format!(
            "{}/api/photos/{b}/decision-note",
            env.server.base_url
        ))
        .json(&json!({"criterion": "sharpness"}))
        .send()
        .await
        .unwrap();

    let rows: Value = env
        .http
        .get(format!(
            "{}/api/taste/decisions?limit=50",
            env.server.base_url
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0]["photo_id"], b);
    assert_eq!(rows[0]["shoot_id"], env.shoot);
    assert_eq!(rows[0]["proposed"], "album");
    assert_eq!(rows[0]["proposed_reason"], "sharp");
    assert_eq!(rows[0]["chosen"], "archive");
    assert_eq!(rows[0]["criterion"], "sharpness");
    assert_eq!(rows[1]["photo_id"], a);
    assert!(rows[1]["criterion"].is_null());

    let one: Value = env
        .http
        .get(format!(
            "{}/api/taste/decisions?limit=1",
            env.server.base_url
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(one.as_array().unwrap().len(), 1);
}
