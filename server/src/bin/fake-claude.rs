//! A stand-in for the `claude` CLI in the helper tests. It never talks to
//! Anthropic: it answers `--version` and `auth status`, and for `-p` it makes
//! the MCP calls a script lists against the server named in `--mcp-config`,
//! printing stream-json lines like `claude -p --output-format stream-json`.
//!
//! `FAKE_CLAUDE_SCRIPT` names a JSON file:
//! `{ "auth": "claude.ai"|"none", "version"?: "2.1.240 (Claude Code)",
//!    "calls": [{"tool", "args"}], "reply"?: "text", "retry_error"?: "rate_limit",
//!    "result": {"is_error", "subtype", "result"}, "exit": i32, "delay_ms": u64 }`.
//! `reply` is a chat answer: it is printed as an assistant text block after the
//! calls, and the calls then print no "Calling x." narration.
//! A string `"$RUN"` anywhere in `args` becomes the `run_id <n>` named in the
//! prompt. `FAKE_CLAUDE_ARGS_LOG`, when set, gets one JSON line per invocation
//! with its argv (program name excluded).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, Default)]
#[serde(default)]
struct Script {
    auth: String,
    version: Option<String>,
    calls: Vec<Call>,
    reply: Option<String>,
    retry_error: Option<String>,
    result: Option<ScriptResult>,
    exit: i32,
    delay_ms: u64,
}

#[derive(Deserialize)]
struct Call {
    tool: String,
    #[serde(default)]
    args: Value,
}

#[derive(Deserialize)]
struct ScriptResult {
    is_error: bool,
    subtype: String,
    result: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(log) = std::env::var_os("FAKE_CLAUDE_ARGS_LOG").filter(|v| !v.is_empty()) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
        {
            let _ = writeln!(f, "{}", serde_json::to_string(&args).unwrap_or_default());
        }
    }
    match run(&args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("fake-claude: {e:#}");
            std::process::exit(2);
        }
    }
}

fn load_script() -> anyhow::Result<Script> {
    let path = std::env::var("FAKE_CLAUDE_SCRIPT").context("FAKE_CLAUDE_SCRIPT is not set")?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {path}"))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).map(String::as_str)
}

fn run(args: &[String]) -> anyhow::Result<i32> {
    let script = load_script()?;
    if args.first().map(String::as_str) == Some("--version") {
        println!(
            "{}",
            script.version.as_deref().unwrap_or("2.1.240 (Claude Code)")
        );
        return Ok(0);
    }
    if args.len() >= 2 && args[0] == "auth" && args[1] == "status" {
        let logged_in = script.auth != "none";
        println!(
            "{}",
            json!({"loggedIn": logged_in, "authMethod": script.auth, "subscriptionType": "pro"})
        );
        return Ok(if logged_in { 0 } else { 1 });
    }
    let prompt = flag(args, "-p").ok_or_else(|| anyhow!("no -p prompt"))?;
    let run_id = run_id_in(prompt);
    let mcp: Value = serde_json::from_str(flag(args, "--mcp-config").unwrap_or("{}"))?;
    let url = mcp["mcpServers"]["photosift"]["url"]
        .as_str()
        .ok_or_else(|| anyhow!("no photosift server in --mcp-config"))?
        .to_string();
    let session = flag(args, "--session-id")
        .or_else(|| flag(args, "--resume"))
        .unwrap_or("fake-session")
        .to_string();

    emit(json!({
        "type": "system", "subtype": "init", "session_id": session,
        "tools": ["mcp__photosift__propose", "mcp__photosift__finish_run"],
        "mcp_servers": [{"name": "photosift", "status": "connected"}],
    }));
    std::thread::sleep(Duration::from_millis(script.delay_ms));
    for (i, call) in script.calls.iter().enumerate() {
        let mut args = call.args.clone();
        if let Some(run) = run_id {
            substitute_run(&mut args, run);
        }
        let id = format!("toolu_{i}");
        let mut content = Vec::new();
        if script.reply.is_none() {
            content.push(json!({"type": "text", "text": format!("Calling {}.", call.tool)}));
        }
        content.push(json!({"type": "tool_use", "id": id,
                            "name": format!("mcp__photosift__{}", call.tool), "input": args}));
        emit(json!({"type": "assistant", "message": {"role": "assistant", "content": content}}));
        let (is_error, text) = match mcp_call(&url, &call.tool, &args) {
            Ok(r) => (
                r["isError"].as_bool().unwrap_or(false),
                r["content"][0]["text"].as_str().unwrap_or("").to_string(),
            ),
            Err(e) => (true, format!("{e:#}")),
        };
        emit(
            json!({"type": "user", "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "is_error": is_error, "content": text},
            ]}}),
        );
    }
    if let Some(reply) = &script.reply {
        emit(
            json!({"type": "assistant", "message": {"role": "assistant", "content": [
                {"type": "text", "text": reply},
            ]}}),
        );
    }
    if let Some(error) = &script.retry_error {
        emit(
            json!({"type": "system", "subtype": "api_retry", "attempt": 1,
                    "max_retries": 10, "retry_delay_ms": 1000, "error_status": 429,
                    "error": error}),
        );
    }
    let r = script.result.unwrap_or(ScriptResult {
        is_error: false,
        subtype: "success".into(),
        result: "Done.".into(),
    });
    emit(json!({
        "type": "result", "subtype": r.subtype, "is_error": r.is_error, "result": r.result,
        "session_id": session, "num_turns": script.calls.len() + 1,
        "total_cost_usd": 0.0123,
        "usage": {"input_tokens": 10, "cache_creation_input_tokens": 1000,
                  "cache_read_input_tokens": 2000, "output_tokens": 300},
    }));
    Ok(script.exit)
}

fn emit(v: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// The number after the first `run_id ` in the prompt.
fn run_id_in(prompt: &str) -> Option<i64> {
    let rest = &prompt[prompt.find("run_id ")? + "run_id ".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn substitute_run(v: &mut Value, run: i64) {
    match v {
        Value::String(s) if s == "$RUN" => *v = json!(run),
        Value::Array(a) => a.iter_mut().for_each(|x| substitute_run(x, run)),
        Value::Object(o) => o.values_mut().for_each(|x| substitute_run(x, run)),
        _ => {}
    }
}

/// One `tools/call` over plain HTTP/1.1: this binary has no HTTP client and
/// needs only a single request per call.
fn mcp_call(url: &str, tool: &str, args: &Value) -> anyhow::Result<Value> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| anyhow!("not an http URL: {url}"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                      "params": {"name": tool, "arguments": args}})
    .to_string();
    let mut stream = TcpStream::connect(host)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    write!(
        stream,
        "POST /{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )?;
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp)?;
    let resp = String::from_utf8_lossy(&resp);
    let (head, body) = resp
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("malformed HTTP response"))?;
    if !head.starts_with("HTTP/1.1 200") {
        bail!("{}", head.lines().next().unwrap_or(""));
    }
    let v: Value = serde_json::from_str(body).context("MCP response body")?;
    if let Some(e) = v.get("error") {
        bail!("JSON-RPC error: {e}");
    }
    Ok(v["result"].clone())
}
