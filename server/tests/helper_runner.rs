//! The helper runner end to end, against the `fake-claude` binary: it answers
//! `auth status` and `--version`, calls PhotoSift's MCP endpoint as a script
//! says, and prints stream-json the way `claude -p` does.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{spawn_server, spawn_server_with_helper, HelperSetup, TestServer};
use futures_util::StreamExt;
use photosift::db::{
    helper::{self, RunKind, RunStatus},
    open,
    photos::{self, NewPhoto, NewShoot, ShootKind},
    DbPool,
};
use photosift::helper::stream::{parse_line, RunEvent};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-claude");
const WAIT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------- parse_line

#[test]
fn parse_line_cases() {
    let init = r#"{"type":"system","subtype":"init","session_id":"s1","tools":["mcp__photosift__propose","mcp__photosift__get_moment"],"mcp_servers":[{"name":"photosift","status":"connected"}],"model":"claude-sonnet-5","permissionMode":"dontAsk"}"#;
    match parse_line(init) {
        Some(RunEvent::Init { tools, mcp_ok }) => {
            assert_eq!(
                tools,
                vec!["mcp__photosift__propose", "mcp__photosift__get_moment"]
            );
            assert!(mcp_ok);
        }
        other => panic!("{other:?}"),
    }
    let failed_mcp = r#"{"type":"system","subtype":"init","tools":[],"mcp_servers":[{"name":"photosift","status":"failed"}]}"#;
    assert!(matches!(
        parse_line(failed_mcp),
        Some(RunEvent::Init { mcp_ok: false, .. })
    ));

    let text = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Looking at moment 0."},{"type":"tool_use","id":"t1","name":"mcp__photosift__get_moment","input":{}}]}}"#;
    match parse_line(text) {
        Some(RunEvent::AssistantText(t)) => assert_eq!(t, "Looking at moment 0."),
        other => panic!("{other:?}"),
    }
    let tool_only = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"x","input":{}}]}}"#;
    assert!(parse_line(tool_only).is_none());

    let retry = r#"{"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":4000,"error_status":429,"error":"rate_limit"}"#;
    match parse_line(retry) {
        Some(RunEvent::Retry { attempt, error }) => {
            assert_eq!(attempt, 2);
            assert_eq!(error, "rate_limit");
        }
        other => panic!("{other:?}"),
    }

    let result = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":21000,"num_turns":12,"result":"Done.","session_id":"22b91cfc-e429-4579-8676-85e272dffbd5","total_cost_usd":0.0566,"usage":{"input_tokens":6,"cache_creation_input_tokens":8197,"cache_read_input_tokens":11942,"output_tokens":2058}}"#;
    match parse_line(result) {
        Some(RunEvent::Result(r)) => {
            assert!(!r.is_error);
            assert_eq!(r.subtype, "success");
            assert_eq!(r.result, "Done.");
            assert!((r.usage.cost_usd - 0.0566).abs() < 1e-9);
            assert_eq!(r.usage.input_tokens, 6 + 8197);
            assert_eq!(r.usage.cache_read_tokens, 11942);
            assert_eq!(r.usage.output_tokens, 2058);
            assert_eq!(r.usage.num_turns, 12);
            assert_eq!(r.usage.session_id, "22b91cfc-e429-4579-8676-85e272dffbd5");
        }
        other => panic!("{other:?}"),
    }
    let error_result = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":1,"session_id":"s","total_cost_usd":0,"usage":{}}"#;
    match parse_line(error_result) {
        Some(RunEvent::Result(r)) => {
            assert!(r.is_error);
            assert_eq!(r.subtype, "error_during_execution");
            assert_eq!(r.result, "");
        }
        other => panic!("{other:?}"),
    }

    for garbage in [
        "",
        "not json",
        "[1,2]",
        r#"{"type":"user","message":{"content":[]}}"#,
        r#"{"type":"stream_event"}"#,
        r#"{"no_type":true}"#,
    ] {
        assert!(parse_line(garbage).is_none(), "{garbage}");
    }
}

// ---------------------------------------------------------------- harness

struct Env {
    server: TestServer,
    _library: TempDir,
    scratch: TempDir,
    script: PathBuf,
    args_log: PathBuf,
    pool: DbPool,
    http: reqwest::Client,
}

impl Env {
    /// A server whose helper runs `bin` (default: the fake) with `script`.
    async fn new(script: Value, edit: impl FnOnce(&mut photosift::config::Settings)) -> Env {
        Self::with_bin(None, script, edit).await
    }

    async fn with_bin(
        bin: Option<PathBuf>,
        script: Value,
        edit: impl FnOnce(&mut photosift::config::Settings),
    ) -> Env {
        let library = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let script_path = scratch.path().join("script.json");
        let args_log = scratch.path().join("args.jsonl");
        std::fs::write(&script_path, script.to_string()).unwrap();
        let server = spawn_server_with_helper(
            library.path(),
            edit,
            HelperSetup {
                claude_bin: bin.unwrap_or_else(|| PathBuf::from(FAKE)),
                env: vec![
                    (
                        "FAKE_CLAUDE_SCRIPT".into(),
                        script_path.to_string_lossy().into_owned(),
                    ),
                    (
                        "FAKE_CLAUDE_ARGS_LOG".into(),
                        args_log.to_string_lossy().into_owned(),
                    ),
                ],
            },
        )
        .await;
        let pool = open(&server.data_root.path().join("photosift.db")).unwrap();
        Env {
            server,
            _library: library,
            scratch,
            script: script_path,
            args_log,
            pool,
            http: reqwest::Client::new(),
        }
    }

    fn url(&self, p: &str) -> String {
        format!("{}{p}", self.server.base_url)
    }

    fn set_script(&self, script: Value) {
        std::fs::write(&self.script, script.to_string()).unwrap();
    }

    async fn ws(&self) -> Ws {
        let url = self.url("/ws").replacen("http://", "ws://", 1);
        tokio_tungstenite::connect_async(url).await.unwrap().0
    }

    async fn get(&self, p: &str) -> Value {
        let resp = self.http.get(self.url(p)).send().await.unwrap();
        assert_eq!(resp.status(), 200, "{p}");
        resp.json().await.unwrap()
    }

    async fn status(&self) -> Value {
        self.get("/api/status").await
    }

    /// Polls `/api/status` until the helper's availability is known.
    async fn helper_status(&self) -> Value {
        for _ in 0..200 {
            let st = self.status().await;
            let h = st["helper"].clone();
            if h["available"] == true || !h["reason"].as_str().unwrap_or("").contains("checking") {
                return h;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("helper availability never settled");
    }

    /// Scans `files` (name, EXIF time) as a card, imports every event with
    /// `kind`, and waits for `enrich_done` on every shoot it touched.
    async fn import(&self, ws: &mut Ws, files: &[(&str, &str)], kind: &str) -> Vec<i64> {
        let card = tempfile::tempdir_in(self.scratch.path()).unwrap();
        for (name, taken) in files {
            let path = card.path().join("DCIM/100_FUJI").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
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
                ),
            )
            .unwrap();
        }
        let scan: Value = self
            .http
            .post(self.url("/api/scan"))
            .json(&json!({ "source": card.path() }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let events: Vec<Value> = scan["events"]
            .as_array()
            .unwrap_or_else(|| panic!("{scan}"))
            .iter()
            .map(|e| {
                json!({
                    "item_idxs": e["item_idxs"],
                    "kind": kind,
                    "name": if kind == "named" { json!("Helper Test") } else { json!("") },
                })
            })
            .collect();
        let resp = self
            .http
            .post(self.url("/api/import"))
            .json(&json!({ "scan_id": scan["scan_id"], "events": events, "camera_offsets": {} }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job_id = resp.json::<Value>().await.unwrap()["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut shoots: Option<Vec<i64>> = None;
        let mut enriched = std::collections::HashSet::new();
        next_events(ws, |v| {
            match v["type"].as_str() {
                Some("import_done") if v["job_id"] == job_id.as_str() => {
                    shoots = Some(
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
            shoots
                .as_ref()
                .is_some_and(|ids| ids.iter().all(|id| enriched.contains(id)))
        })
        .await;
        shoots.unwrap()
    }

    fn run_row(&self, run: i64) -> helper::RunRow {
        helper::get_run(&self.pool, run).unwrap().unwrap()
    }

    /// Each logged invocation's argv (without the program name).
    fn invocations(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(&self.args_log)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Invocations that started a run (not `auth status` / `--version`).
    fn run_invocations(&self) -> Vec<Vec<String>> {
        self.invocations()
            .into_iter()
            .filter(|a| a.iter().any(|x| x == "-p"))
            .collect()
    }
}

/// Feeds every JSON event to `f` until it returns true.
async fn next_events(ws: &mut Ws, mut f: impl FnMut(&Value) -> bool) {
    tokio::time::timeout(WAIT, async {
        while let Some(msg) = ws.next().await {
            let Ok(text) = msg.unwrap().into_text() else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if f(&v) {
                return;
            }
        }
        panic!("websocket closed early");
    })
    .await
    .expect("timed out waiting for events");
}

/// Waits for a `helper_status` event with `status` and returns it.
async fn wait_helper(ws: &mut Ws, status: &str) -> Value {
    let mut found = Value::Null;
    next_events(ws, |v| {
        if v["type"] == "helper_status" {
            if v["status"] == "failed" && status != "failed" {
                panic!("helper run failed: {v}");
            }
            if v["status"] == status {
                found = v.clone();
                return true;
            }
        }
        false
    })
    .await;
    found
}

/// The `-p` prompt of an invocation.
fn prompt_of(args: &[String]) -> &str {
    let i = args.iter().position(|a| a == "-p").unwrap();
    &args[i + 1]
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == name)?;
    Some(args.get(i + 1).map_or("", String::as_str))
}

fn script(calls: Value) -> Value {
    json!({
        "auth": "claude.ai",
        "calls": calls,
        "result": {"is_error": false, "subtype": "success", "result": "All proposed."},
        "exit": 0,
        "delay_ms": 0,
    })
}

const THREE: &[(&str, &str)] = &[
    ("DSCF0001.JPG", "2026:10:18 10:00:00"),
    ("DSCF0002.JPG", "2026:10:18 10:01:00"),
    ("DSCF0003.JPG", "2026:10:18 10:02:00"),
];

fn album_two_and_finish() -> Value {
    script(json!([
        {"tool": "propose", "args": {"run_id": "$RUN", "items": [
            {"photo_id": 1, "outcome": "album", "reason": "big laugh, eyes open"},
            {"photo_id": 2, "outcome": "album", "reason": "whole family in frame"},
        ]}},
        {"tool": "finish_run", "args": {"run_id": "$RUN", "summary": "A short morning. Two keepers."}},
    ]))
}

/// Waits until the runner has recorded the usage `claude -p` reported, which
/// lands just after `finish_run` set the run `done`.
async fn wait_usage(env: &Env, run: i64) -> helper::RunRow {
    for _ in 0..200 {
        let row = env.run_row(run);
        if row.cost_usd.is_some() {
            return row;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("usage never recorded for run {run}");
}

// ---------------------------------------------------------------- end to end

#[tokio::test]
async fn background_run_after_enrichment() {
    let env = Env::new(album_two_and_finish(), |_| {}).await;
    let mut ws = env.ws().await;
    let shoots = env.import(&mut ws, THREE, "named").await;
    let done = wait_helper(&mut ws, "done").await;
    assert_eq!(done["shoot_id"], shoots[0], "{done}");
    let run = done["run_id"].as_i64().unwrap();

    let review = env.get(&format!("/api/shoots/{}/review", shoots[0])).await;
    let h = &review["helper"];
    assert_eq!(h["status"], "done", "{h}");
    assert_eq!(h["summary"], "A short morning. Two keepers.");
    assert_eq!(h["suggested"]["album"], 2, "{h}");
    assert_eq!(review["photos"]["1"]["proposal"]["outcome"], "album");
    assert_eq!(review["photos"]["3"]["proposal"], Value::Null);
    // A proposal is never the outcome.
    assert_eq!(review["photos"]["1"]["outcome"], "unreviewed");

    let row = wait_usage(&env, run).await;
    assert_eq!(row.kind, "background");
    assert!(row.cost_usd.unwrap() > 0.0, "{row:?}");
    assert!(row.num_turns.unwrap() > 0, "{row:?}");
    let version = row.prompt_version.clone().unwrap();
    assert_eq!(version.len(), 12, "{version}");
    assert_eq!(version, *photosift::helper::prompt::PROMPT_VERSION);
    assert_eq!(row.model.as_deref(), Some("sonnet"));
    assert!(row.session_id.is_some(), "{row:?}");
    let system = env
        .server
        .data_root
        .path()
        .join("helper")
        .join("runs")
        .join(run.to_string())
        .join("system.md");
    let system = std::fs::read_to_string(&system).unwrap();
    assert!(system.contains("No notes yet."), "{system}");
    assert!(!system.contains("{{"), "unfilled placeholder in {system}");

    let st = env.status().await;
    assert_eq!(st["helper"]["available"], true, "{st}");
    assert_eq!(st["helper"]["transport"], "cli");
}

#[tokio::test]
async fn argv_is_isolated_and_denies_writes() {
    let env = Env::new(album_two_and_finish(), |_| {}).await;
    let mut ws = env.ws().await;
    env.import(&mut ws, THREE, "named").await;
    wait_helper(&mut ws, "done").await;

    let runs = env.run_invocations();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let a = &runs[0];
    for f in [
        "--strict-mcp-config",
        "--verbose",
        "--disable-slash-commands",
    ] {
        assert!(a.iter().any(|x| x == f), "{f} missing from {a:?}");
    }
    assert_eq!(flag(a, "--tools"), Some(""), "{a:?}");
    assert_eq!(flag(a, "--output-format"), Some("stream-json"));
    assert_eq!(flag(a, "--permission-mode"), Some("dontAsk"));
    assert_eq!(flag(a, "--setting-sources"), Some("local"));
    assert_eq!(flag(a, "--allowedTools"), Some("mcp__photosift__*"));
    assert_eq!(flag(a, "--model"), Some("sonnet"));
    assert_eq!(flag(a, "--max-budget-usd"), Some("3"));
    assert_eq!(flag(a, "--max-turns"), Some("120"));
    let denied = flag(a, "--disallowedTools").unwrap();
    for tool in [
        "mcp__photosift__set_outcome",
        "mcp__photosift__set_taste_profile",
        "mcp__photosift__start_run",
    ] {
        assert!(denied.contains(tool), "{tool} not denied: {denied}");
    }
    let mcp: Value = serde_json::from_str(flag(a, "--mcp-config").unwrap()).unwrap();
    let url = mcp["mcpServers"]["photosift"]["url"].as_str().unwrap();
    assert_eq!(url, format!("{}/mcp", env.server.base_url));
    assert!(flag(a, "--session-id").is_some_and(|s| s.len() == 36));
    assert!(flag(a, "--system-prompt-file").is_some());
    assert!(!a.iter().any(|x| x == "--bare"), "{a:?}");
    // The fake reports 2.1.240, which predates --permission-prompts.
    assert!(!a.iter().any(|x| x == "--permission-prompts"), "{a:?}");
    assert!(prompt_of(a).contains("run_id"), "{a:?}");
}

#[tokio::test]
async fn new_cli_gets_permission_prompts_none() {
    let mut s = album_two_and_finish();
    s["version"] = json!("2.1.259 (Claude Code)");
    let env = Env::new(s, |_| {}).await;
    let mut ws = env.ws().await;
    env.import(&mut ws, THREE, "named").await;
    wait_helper(&mut ws, "done").await;
    let a = &env.run_invocations()[0];
    assert_eq!(flag(a, "--permission-prompts"), Some("none"), "{a:?}");
}

#[tokio::test]
async fn cli_missing_reports_unavailable() {
    let env = Env::with_bin(
        Some(PathBuf::from(r"C:\definitely\not\here\claude.exe")),
        album_two_and_finish(),
        |_| {},
    )
    .await;
    let h = env.helper_status().await;
    assert_eq!(h["available"], false, "{h}");
    assert!(h["reason"].as_str().unwrap().contains("not found"), "{h}");

    let mut ws = env.ws().await;
    let shoots = env.import(&mut ws, THREE, "named").await;
    let review = env.get(&format!("/api/shoots/{}/review", shoots[0])).await;
    assert_eq!(review["shoot"]["enriched"], true);
    assert_eq!(review["helper"]["status"], "unavailable", "{review}");
    let resp = env
        .http
        .post(env.url(&format!("/api/shoots/{}/helper/run", shoots[0])))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"].as_str().unwrap().contains("not found"),
        "{body}"
    );
    let n: i64 = env
        .pool
        .with(|c| c.query_row("SELECT COUNT(*) FROM helper_runs", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(n, 0, "no run is created while the helper is unavailable");
}

#[tokio::test]
async fn logged_out_reports_unavailable() {
    let mut s = album_two_and_finish();
    s["auth"] = json!("none");
    let env = Env::new(s, |_| {}).await;
    let h = env.helper_status().await;
    assert_eq!(h["available"], false, "{h}");
    assert!(
        h["reason"].as_str().unwrap().contains("claude auth login"),
        "{h}"
    );
}

#[tokio::test]
async fn rate_limit_result_fails_run_keeps_proposals() {
    let s = json!({
        "auth": "claude.ai",
        "calls": [{"tool": "propose", "args": {"run_id": "$RUN", "items": [
            {"photo_id": 2, "outcome": "album", "reason": "the cake moment"},
        ]}}],
        "retry_error": "rate_limit",
        "result": {"is_error": true, "subtype": "error_during_execution",
                   "result": "API Error: rate limited"},
        "exit": 1,
        "delay_ms": 0,
    });
    let env = Env::new(s, |_| {}).await;
    let mut ws = env.ws().await;
    let shoots = env.import(&mut ws, THREE, "named").await;
    let failed = wait_helper(&mut ws, "failed").await;
    let error = failed["error"].as_str().unwrap();
    assert!(error.contains("usage limit"), "{error}");

    let review = env.get(&format!("/api/shoots/{}/review", shoots[0])).await;
    assert_eq!(review["helper"]["status"], "failed");
    assert!(review["helper"]["error"]
        .as_str()
        .unwrap()
        .contains("usage limit"));
    assert_eq!(
        review["photos"]["2"]["proposal"]["reason"],
        "the cake moment"
    );
    assert_eq!(review["helper"]["suggested"]["album"], 1);
}

#[tokio::test]
async fn run_ending_without_finish_run_fails() {
    let s = script(json!([
        {"tool": "propose", "args": {"run_id": "$RUN", "items": [
            {"photo_id": 1, "outcome": "archive", "reason": "soft focus"},
        ]}},
    ]));
    let env = Env::new(s, |_| {}).await;
    let mut ws = env.ws().await;
    env.import(&mut ws, THREE, "named").await;
    let failed = wait_helper(&mut ws, "failed").await;
    assert!(
        failed["error"].as_str().unwrap().contains("finish_run"),
        "{failed}"
    );
    let row = wait_usage(&env, failed["run_id"].as_i64().unwrap()).await;
    assert_eq!(row.status, "failed");
}

#[tokio::test]
async fn startup_marks_running_runs_interrupted() {
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
    let photo = photos::insert_photo(
        &pool,
        &NewPhoto {
            shoot_id: shoot,
            jpeg_path: Some(folder.path().join("a.jpg").to_string_lossy().into_owned()),
            sha256: "hash-a".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let running = helper::create_run(&pool, shoot, RunKind::Background, None, None).unwrap();
    helper::set_run_status(&pool, running, RunStatus::Running, None).unwrap();
    let queued = helper::create_run(&pool, shoot, RunKind::Background, None, None).unwrap();

    let server = common::respawn_server(server).await;
    for run in [running, queued] {
        let row = helper::get_run(&pool, run).unwrap().unwrap();
        assert_eq!(row.status, "interrupted", "{row:?}");
        assert!(row.finished_at.is_some());
    }

    // An orphaned `claude` from before the restart can no longer write.
    let scratch = tempfile::tempdir().unwrap();
    let script_path = scratch.path().join("script.json");
    std::fs::write(
        &script_path,
        script(
            json!([{"tool": "propose", "args": {"run_id": "$RUN", "items": [
            {"photo_id": photo, "outcome": "album", "reason": "late write"}]}}]),
        )
        .to_string(),
    )
    .unwrap();
    let mcp = json!({"mcpServers": {"photosift": {"type": "http",
        "url": format!("{}/mcp", server.base_url)}}});
    let mut cmd = std::process::Command::new(FAKE);
    cmd.args([
        "-p",
        &format!("Shoot {shoot}, run_id {running}."),
        "--mcp-config",
        &mcp.to_string(),
        "--session-id",
        "00000000-0000-4000-8000-000000000000",
    ])
    .env("FAKE_CLAUDE_SCRIPT", &script_path);
    // Off the test's runtime thread, which also serves the fake's MCP calls.
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let tool_result = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["type"] == "user")
        .unwrap_or_else(|| panic!("no tool result in {stdout}"));
    let block = &tool_result["message"]["content"][0];
    assert_eq!(block["is_error"], true, "{block}");
    assert!(
        block["content"]
            .as_str()
            .unwrap()
            .contains(&format!("run {running} is not running")),
        "{block}"
    );
    let n: i64 = pool
        .with(|c| c.query_row("SELECT COUNT(*) FROM proposals", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn second_import_queues_run_for_new_photos() {
    let first = script(json!([
        {"tool": "propose", "args": {"run_id": "$RUN", "items": [
            {"photo_id": 1, "outcome": "album", "reason": "first light"},
            {"photo_id": 2, "outcome": "library", "reason": "nice but similar"},
            {"photo_id": 3, "outcome": "archive", "reason": "blurred"},
        ]}},
        {"tool": "finish_run", "args": {"run_id": "$RUN", "summary": "Three everyday frames."}},
    ]));
    let env = Env::new(first, |_| {}).await;
    let mut ws = env.ws().await;
    let a = env.import(&mut ws, THREE, "everyday").await;
    let run1 = wait_helper(&mut ws, "done").await["run_id"]
        .as_i64()
        .unwrap();
    wait_usage(&env, run1).await;

    env.set_script(script(json!([
        {"tool": "finish_run", "args": {"run_id": "$RUN", "summary": "Two more."}},
    ])));
    let b = env
        .import(
            &mut ws,
            &[
                ("DSCF0004.JPG", "2026:10:19 09:00:00"),
                ("DSCF0005.JPG", "2026:10:19 09:01:00"),
            ],
            "everyday",
        )
        .await;
    assert_eq!(a, b, "both imports land in the month's Everyday shoot");
    let run2 = wait_helper(&mut ws, "done").await["run_id"]
        .as_i64()
        .unwrap();
    assert_ne!(run1, run2);

    let runs = env.run_invocations();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert!(
        !prompt_of(&runs[0]).contains("new since the last run"),
        "{}",
        prompt_of(&runs[0])
    );
    let p2 = prompt_of(&runs[1]);
    assert!(p2.contains(&format!("run_id {run2}")), "{p2}");
    let listed = p2
        .split("new since the last run: ")
        .nth(1)
        .unwrap_or_else(|| panic!("{p2}"))
        .split('.')
        .next()
        .unwrap();
    let ids: Vec<i64> = listed
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    assert_eq!(ids, vec![4, 5], "{p2}");

    let review = env.get(&format!("/api/shoots/{}/review", a[0])).await;
    for (id, outcome) in [(1, "album"), (2, "library"), (3, "archive")] {
        assert_eq!(
            review["photos"][id.to_string()]["proposal"]["outcome"],
            outcome
        );
    }
    let still_run1: i64 = env
        .pool
        .with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM proposals WHERE run_id = ?1",
                [run1],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(still_run1, 3);
}

#[tokio::test]
async fn manual_run_conflicts_while_running() {
    let mut s = script(json!([
        {"tool": "finish_run", "args": {"run_id": "$RUN", "summary": "Looked again."}},
    ]));
    s["delay_ms"] = json!(2000);
    // R3: helper_enabled gates only the automatic run; the button still works.
    let env = Env::new(s, |s| s.helper_enabled = false).await;
    let mut ws = env.ws().await;
    let shoots = env.import(&mut ws, THREE, "named").await;
    assert!(env.run_invocations().is_empty(), "no automatic run");
    assert_eq!(
        env.pool
            .with(
                |c| c.query_row("SELECT COUNT(*) FROM helper_runs", [], |r| r
                    .get::<_, i64>(0))
            )
            .unwrap(),
        0
    );

    let url = env.url(&format!("/api/shoots/{}/helper/run", shoots[0]));
    let first = env.http.post(&url).send().await.unwrap();
    assert_eq!(first.status(), 202);
    let run = first.json::<Value>().await.unwrap()["run_id"]
        .as_i64()
        .unwrap();
    let second = env.http.post(&url).send().await.unwrap();
    assert_eq!(second.status(), 409);
    wait_helper(&mut ws, "done").await;
    let row = env.run_row(run);
    assert_eq!(row.summary.as_deref(), Some("Looked again."));
    // Once it is done, another manual run is accepted.
    let third = env.http.post(&url).send().await.unwrap();
    assert_eq!(third.status(), 202);

    let missing = env
        .http
        .post(env.url("/api/shoots/9999/helper/run"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn cmd_shim_runs_through_cmd() {
    let shim_dir = tempfile::tempdir().unwrap();
    let shim = shim_dir.path().join("claude.cmd");
    std::fs::write(&shim, format!("@\"{FAKE}\" %*\r\n")).unwrap();
    let env = Env::with_bin(Some(shim), album_two_and_finish(), |_| {}).await;
    let h = env.helper_status().await;
    assert_eq!(h["available"], true, "{h}");
    let mut ws = env.ws().await;
    env.import(&mut ws, THREE, "named").await;
    wait_helper(&mut ws, "done").await;
    let a = &env.run_invocations()[0];
    // The JSON and the empty --tools value survive cmd.exe intact.
    assert_eq!(flag(a, "--tools"), Some(""), "{a:?}");
    serde_json::from_str::<Value>(flag(a, "--mcp-config").unwrap()).unwrap();
}

#[tokio::test]
async fn automatic_follow_up_with_nothing_new_spawns_no_claude() {
    // Run 1 waits long enough for a second import to land, then proposes all
    // five photos, so the follow-up queued by the second EnrichDone has
    // nothing left to do.
    let mut s = script(json!([
        {"tool": "propose", "args": {"run_id": "$RUN", "items": [
            {"photo_id": 1, "outcome": "album", "reason": "first light"},
            {"photo_id": 2, "outcome": "archive", "reason": "same pose"},
            {"photo_id": 3, "outcome": "archive", "reason": "blurred"},
            {"photo_id": 4, "outcome": "library", "reason": "nice later frame"},
            {"photo_id": 5, "outcome": "archive", "reason": "duplicate"},
        ]}},
        {"tool": "finish_run", "args": {"run_id": "$RUN", "summary": "Five everyday frames."}},
    ]));
    s["delay_ms"] = json!(5000);
    let env = Env::new(s, |_| {}).await;
    let mut ws = env.ws().await;
    env.import(&mut ws, THREE, "everyday").await;
    wait_helper(&mut ws, "running").await;
    env.import(
        &mut ws,
        &[
            ("DSCF0004.JPG", "2026:10:19 09:00:00"),
            ("DSCF0005.JPG", "2026:10:19 09:01:00"),
        ],
        "everyday",
    )
    .await;
    let done = wait_helper(&mut ws, "done").await;
    let run1 = done["run_id"].as_i64().unwrap();
    // Let the worker take and settle the follow-up job.
    for _ in 0..200 {
        if env.status().await["helper"]["queue"] == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(env.status().await["helper"]["queue"], 0);

    assert_eq!(
        env.run_invocations().len(),
        1,
        "{:?}",
        env.run_invocations()
    );
    let runs: Vec<(i64, String)> = env
        .pool
        .with(|c| {
            let mut st = c.prepare("SELECT id, status FROM helper_runs ORDER BY id")?;
            let rows = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>();
            rows
        })
        .unwrap();
    assert_eq!(
        runs,
        vec![(run1, "done".to_string())],
        "no dangling run row"
    );
    let review = env.get("/api/shoots/1/review").await;
    assert_eq!(review["helper"]["status"], "done");
    assert_eq!(review["helper"]["summary"], "Five everyday frames.");
}
