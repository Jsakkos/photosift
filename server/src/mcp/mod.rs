//! MCP endpoint: the Streamable HTTP transport, stateless subset. Each POST
//! carries one JSON-RPC message and is answered with `application/json`; there
//! are no sessions and no server-initiated stream (`GET` is 405), which MCP
//! clients accept from a server that never needs to push.

pub mod tools;

use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::api::AppState;
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

pub async fn post(State(state): State<AppState>, body: Bytes) -> Response {
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
    match dispatch(&state, method, params).await {
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

async fn dispatch(state: &AppState, method: &str, params: Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "photosift", "version": env!("CARGO_PKG_VERSION")},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": tools::all().iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.schema,
            })).collect::<Vec<_>>(),
        })),
        "tools/call" => call_tool(state, params).await,
        other => Err((METHOD_NOT_FOUND, format!("unknown method {other}"))),
    }
}

async fn call_tool(state: &AppState, params: Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((INVALID_PARAMS, "tools/call needs a tool name".to_string()))?;
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
