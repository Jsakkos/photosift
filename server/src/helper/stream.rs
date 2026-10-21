//! `claude -p --output-format stream-json --verbose`: one JSON object per line.
//! Only the events the runner acts on are parsed; everything else is `None`.

use serde_json::Value;

use crate::db::helper::RunUsage;

#[derive(Debug, Clone, PartialEq)]
pub enum RunEvent {
    /// `system/init`. `mcp_ok` is whether PhotoSift's MCP server connected.
    Init {
        tools: Vec<String>,
        mcp_ok: bool,
    },
    /// The text blocks of one assistant message, joined.
    AssistantText(String),
    /// `system/api_retry`: the API refused a request and the CLI will retry.
    /// `error` is e.g. `rate_limit`.
    Retry {
        attempt: u32,
        error: String,
    },
    Result(RunResult),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    pub is_error: bool,
    /// `success`, `error_max_turns`, `error_during_execution`, ...
    pub subtype: String,
    /// The final assistant text (empty on most errors).
    pub result: String,
    pub usage: RunUsage,
}

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

/// Parses one stdout line. Pure: unknown or malformed lines are `None`.
pub fn parse_line(line: &str) -> Option<RunEvent> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    match (v["type"].as_str()?, v["subtype"].as_str()) {
        ("system", Some("init")) => Some(RunEvent::Init {
            tools: v["tools"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            mcp_ok: v["mcp_servers"].as_array().is_some_and(|servers| {
                servers
                    .iter()
                    .any(|s| s["name"] == "photosift" && s["status"] == "connected")
            }),
        }),
        ("system", Some("api_retry")) => Some(RunEvent::Retry {
            attempt: u32::try_from(int(&v["attempt"])).unwrap_or(0),
            error: v["error"].as_str().unwrap_or("unknown").to_string(),
        }),
        ("assistant", _) => {
            let text: Vec<&str> = v["message"]["content"]
                .as_array()?
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect();
            (!text.is_empty()).then(|| RunEvent::AssistantText(text.join("\n")))
        }
        ("result", subtype) => {
            let u = &v["usage"];
            Some(RunEvent::Result(RunResult {
                is_error: v["is_error"].as_bool().unwrap_or(true),
                subtype: subtype.unwrap_or("").to_string(),
                result: v["result"].as_str().unwrap_or("").to_string(),
                usage: RunUsage {
                    cost_usd: v["total_cost_usd"].as_f64().unwrap_or(0.0),
                    input_tokens: int(&u["input_tokens"]) + int(&u["cache_creation_input_tokens"]),
                    output_tokens: int(&u["output_tokens"]),
                    cache_read_tokens: int(&u["cache_read_input_tokens"]),
                    num_turns: int(&v["num_turns"]),
                    session_id: v["session_id"].as_str().unwrap_or("").to_string(),
                },
            }))
        }
        _ => None,
    }
}
