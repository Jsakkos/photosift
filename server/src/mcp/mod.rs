//! MCP endpoint: the Streamable HTTP transport, stateless subset. Each POST
//! carries one JSON-RPC message and is answered with `application/json`; there
//! are no sessions and no server-initiated stream (`GET` is 405), which MCP
//! clients accept from a server that never needs to push.

pub mod tools;

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::AppState;
use crate::helper::transport::HelperClient;
use tools::{Tool, ToolResult};

const PROTOCOL_VERSION: &str = "2025-06-18";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

/// `GET /mcp`: this server never opens a stream.
pub async fn get_not_allowed() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response()
}

/// `?client=helper&kind=<background|chat|eval>&token=<token>` marks a child
/// the helper started; plain `/mcp` is an external client (Claude Desktop,
/// Claude Code), which may do what the HTTP API does.
#[derive(Deserialize)]
pub struct ClientQuery {
    client: Option<String>,
    kind: Option<String>,
    token: Option<String>,
}

/// Who is calling: the user's own MCP client, or a run the app started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Caller {
    External,
    Helper(HelperClient),
}

fn caller(state: &AppState, q: &ClientQuery) -> Result<Caller, (StatusCode, &'static str)> {
    let Some(client) = q.client.as_deref() else {
        return Ok(Caller::External);
    };
    if client != "helper" {
        return Err((StatusCode::BAD_REQUEST, "unknown MCP client"));
    }
    let kind = q
        .kind
        .as_deref()
        .and_then(HelperClient::parse)
        .ok_or((StatusCode::BAD_REQUEST, "unknown helper kind"))?;
    match q.token.as_deref() {
        Some(t) if state.helper.is_own_token(t) => Ok(Caller::Helper(kind)),
        _ => Err((StatusCode::FORBIDDEN, "not a helper this app started")),
    }
}

pub async fn post(
    State(state): State<AppState>,
    Query(q): Query<ClientQuery>,
    body: Bytes,
) -> Response {
    let caller = match caller(&state, &q) {
        Ok(c) => c,
        Err((status, msg)) => return (status, Json(json!({ "error": msg }))).into_response(),
    };
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return error_response(Value::Null, PARSE_ERROR, format!("invalid JSON: {e}")),
    };
    if !msg.is_object() {
        // 2025-06-18 dropped JSON-RPC batching.
        return error_response(
            Value::Null,
            INVALID_REQUEST,
            "expected a single JSON-RPC message object",
        );
    }
    let id = msg.get("id").cloned();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        // A response to a server request (we send none) or garbage.
        return if id.is_some() && (msg.get("result").is_some() || msg.get("error").is_some()) {
            StatusCode::ACCEPTED.into_response()
        } else {
            error_response(id.unwrap_or(Value::Null), INVALID_REQUEST, "missing method")
        };
    };
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    match dispatch(&state, caller, method, params).await {
        Ok(result) => Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response(),
        Err((code, message)) => error_response(id, code, message),
    }
}

fn error_response(id: Value, code: i64, message: impl Into<String>) -> Response {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message.into()},
    }))
    .into_response()
}

fn allowed(caller: Caller, tool: &str) -> bool {
    match caller {
        Caller::External => true,
        Caller::Helper(kind) => kind.may_call(tool),
    }
}

async fn dispatch(
    state: &AppState,
    caller: Caller,
    method: &str,
    params: Value,
) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "photosift", "version": env!("CARGO_PKG_VERSION")},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": tools::all().iter().filter(|t| allowed(caller, t.name)).map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.schema,
            })).collect::<Vec<_>>(),
        })),
        "tools/call" => call_tool(state, caller, params).await,
        other => Err((METHOD_NOT_FOUND, format!("unknown method {other}"))),
    }
}

async fn call_tool(
    state: &AppState,
    caller: Caller,
    params: Value,
) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((INVALID_PARAMS, "tools/call needs a tool name".to_string()))?;
    if !allowed(caller, name) {
        crate::plog!("mcp", "refused {name} to a helper run");
        return Ok(ToolResult::error(format!(
            "{name} is not available to the helper: the user decides outcomes, \
             and the app opens its own runs"
        ))
        .to_json());
    }
    let tool: Tool = tools::all()
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| (INVALID_PARAMS, format!("unknown tool {name}")))?;
    let args = match params.get("arguments") {
        Some(a) if !a.is_null() => a.clone(),
        _ => json!({}),
    };
    let state = state.clone();
    let result: ToolResult = tokio::task::spawn_blocking(move || (tool.call)(&state, args))
        .await
        .map_err(|e| {
            crate::plog!("mcp", "tool {name} panicked: {e}");
            (INTERNAL_ERROR, format!("tool panicked: {e}"))
        })?;
    Ok(result.to_json())
}
