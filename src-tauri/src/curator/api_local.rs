//! Local OpenAI-compatible provider. Talks to any server that exposes
//! `POST {base_url}/chat/completions` (Ollama, LM Studio, vLLM,
//! llama.cpp server, …).
//!
//! Differences vs. the cloud providers:
//! - No tool calling and no `response_format: json_object` — both are
//!   wildly inconsistent across local servers. LM Studio in particular
//!   silently *hangs* the inference loop when it accepts json_object
//!   for a model whose runtime grammar support is missing (observed
//!   2026-05 with Gemma 4 E4B). Instead we embed the JSON Schema in
//!   the system prompt and parse the model's text content.
//! - Markdown code fences (```json … ```) are stripped before parsing
//!   since smaller models often wrap JSON in them even when told not to.
//! - Cost is always 0 (handled in `cost.rs`); usage is still tracked
//!   when the server returns it so the UI can show token throughput.

use crate::curator::prompts::{
    stage1_system, stage1_user_text, stage2_system, stage2_user_text, CURRENT_PROMPT_VERSION,
};
use crate::curator::provider::{
    ClusterJudgmentBatch, CuratorProvider, Stage2Cluster, SummaryResult,
};
use crate::curator::types::{
    judgments_tool_schema, summary_tool_schema, CuratorJudgment, ShootSummary, Usage,
};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

const PROVIDER_ID: &str = "local";

#[derive(Clone)]
pub struct LocalProvider {
    http: reqwest::Client,
    /// Includes the `/v1` suffix; we append `/chat/completions` etc.
    base_url: String,
    model: String,
    /// Optional bearer token for servers that gate access (some vLLM /
    /// hosted-OpenAI deployments do). None for stock Ollama / LM Studio.
    api_key: Option<String>,
}

impl LocalProvider {
    pub fn new(base_url: String, model: String, api_key: Option<String>) -> Result<Self> {
        if model.trim().is_empty() {
            bail!("local model is empty — set a model name in Settings");
        }
        if base_url.trim().is_empty() {
            bail!("local base URL is empty");
        }
        let http = reqwest::Client::builder()
            // Local inference can be slow on cold start (model load).
            // Bump the timeout above the cloud providers' 120s.
            .timeout(Duration::from_secs(300))
            .build()
            .context("build reqwest client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            api_key,
        })
    }

    fn auth_headers(&self) -> Result<HeaderMap> {
        let mut h = HeaderMap::new();
        h.insert("content-type", HeaderValue::from_static("application/json"));
        if let Some(k) = &self.api_key {
            let v = format!("Bearer {}", k);
            h.insert(
                "authorization",
                HeaderValue::from_str(&v).context("api key contains invalid bytes")?,
            );
        }
        Ok(h)
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }
}

#[async_trait]
impl CuratorProvider for LocalProvider {
    /// Probe `/v1/models` first (the OpenAI-compatible models list); if
    /// that fails with 404 (some servers don't implement it), fall back
    /// to a tiny chat completion to confirm the chat endpoint works.
    async fn test_connection(&self) -> Result<()> {
        let resp = self
            .http
            .get(self.models_url())
            .headers(self.auth_headers()?)
            .send()
            .await
            .context("network error contacting local server")?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        if status == StatusCode::NOT_FOUND {
            // LM Studio in older versions doesn't expose /v1/models.
            // Fall through to a chat probe.
        } else if status == StatusCode::UNAUTHORIZED {
            bail!("local server rejected the API key (401)");
        } else if !status.is_server_error() && status != StatusCode::METHOD_NOT_ALLOWED {
            let body = resp.text().await.unwrap_or_default();
            bail!("non-2xx from /models: {} {}", status, body);
        }

        let probe = ChatRequest {
            model: &self.model,
            messages: vec![ChatMessage::user_text("ping")],
            max_tokens: Some(8),
            stream: false,
        };
        let resp = self
            .http
            .post(self.chat_url())
            .headers(self.auth_headers()?)
            .json(&probe)
            .send()
            .await
            .context("network error contacting local /chat/completions")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("non-2xx from /chat/completions: {} {}", status, body);
        }
        Ok(())
    }

    async fn run_stage1(
        &self,
        photo_count: i64,
        cluster_count: i64,
        thumb_paths: &[&Path],
    ) -> Result<SummaryResult> {
        let system = format!(
            "{}\n\n{}",
            stage1_system(),
            schema_instruction(&summary_tool_schema()),
        );
        let mut user_parts: Vec<ChatPart> =
            vec![ChatPart::text(stage1_user_text(photo_count, cluster_count))];
        for p in thumb_paths {
            user_parts.push(read_thumb_as_part(p)?);
        }

        let req = ChatRequest {
            model: &self.model,
            messages: vec![
                ChatMessage::system_text(&system),
                ChatMessage::user_parts(user_parts),
            ],
            // Reasoning-style local models (Gemma 4, GPT-OSS, R1) emit a
            // long thinking-trace before the JSON. Give plenty of room
            // so the answer doesn't get clipped at finish_reason=length.
            max_tokens: Some(2500),
            stream: false,
        };

        let resp = send_with_retries(&self.http, &self.chat_url(), &self.auth_headers()?, &req)
            .await?;
        let content = first_message_content(&resp)?;
        let stripped = strip_json_fences(&content);
        let candidate = extract_target_json(&stripped, "shoot_type")
            .map(|s| maybe_unwrap_tool_call(s, "record_shoot_summary"))
            .unwrap_or_default();
        let summary: ShootSummary = serde_json::from_str(&candidate).with_context(|| {
            format!(
                "decode local Stage 1 JSON; first 500 chars of model output: {:?}",
                &content[..content.len().min(500)]
            )
        })?;
        Ok(SummaryResult {
            summary,
            usage: usage_from_resp(&resp),
            model: self.model.clone(),
        })
    }

    async fn run_stage2_cluster(
        &self,
        summary: &ShootSummary,
        cluster: &Stage2Cluster<'_>,
    ) -> Result<ClusterJudgmentBatch> {
        let tech_lines: Vec<String> = cluster
            .frames
            .iter()
            .map(|f| {
                crate::curator::prompts::tech_line(
                    f.photo_id,
                    f.sharpness_1_10,
                    f.face_count,
                    f.eyes_open_count,
                    f.smile_score,
                )
            })
            .collect();

        let system = format!(
            "{}\n\n{}",
            stage2_system(summary),
            schema_instruction(&judgments_tool_schema()),
        );
        let mut user_parts: Vec<ChatPart> =
            vec![ChatPart::text(stage2_user_text(cluster.frames.len(), &tech_lines))];
        for f in cluster.frames {
            user_parts.push(read_thumb_as_part(f.thumb_path)?);
        }

        let req = ChatRequest {
            model: &self.model,
            messages: vec![
                ChatMessage::system_text(&system),
                ChatMessage::user_parts(user_parts),
            ],
            // Headroom for reasoning preamble + N judgment objects. We
            // observed truncation mid-object on 4-6 frame clusters at
            // 500/frame; bump to 800/frame + 2000 base. Even a 6-frame
            // cluster only consumes ~6800 tokens, well under 32k context.
            max_tokens: Some(800 * cluster.frames.len() as u32 + 2000),
            stream: false,
        };

        let resp = send_with_retries(&self.http, &self.chat_url(), &self.auth_headers()?, &req)
            .await?;
        let content = first_message_content(&resp)?;
        let stripped = strip_json_fences(&content);
        let candidate = extract_target_json(&stripped, "judgments")
            .map(|s| maybe_unwrap_tool_call(s, "record_judgments"))
            .unwrap_or_default();
        #[derive(Deserialize)]
        struct Wrap {
            judgments: Vec<CuratorJudgment>,
        }
        let wrap: Wrap = serde_json::from_str(&candidate).with_context(|| {
            format!(
                "decode local Stage 2 JSON; first 500 chars of model output: {:?}",
                &content[..content.len().min(500)]
            )
        })?;
        Ok(ClusterJudgmentBatch {
            judgments: wrap.judgments,
            usage: usage_from_resp(&resp),
            model: self.model.clone(),
            prompt_version: CURRENT_PROMPT_VERSION,
        })
    }

    fn provider_id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn concurrency_limit(&self) -> usize {
        // Local inference shares one GPU; parallel calls thrash VRAM
        // and Windows commit on a 24 GB card running a 26B vision model.
        1
    }
}

/// Build the schema-in-prompt instruction. Pretty-printed schema +
/// terse "respond with valid JSON" directive. Smaller models tend to
/// follow this when `response_format: json_object` is also set.
fn schema_instruction(schema: &serde_json::Value) -> String {
    format!(
        "Output a single JSON object that conforms exactly to this JSON Schema. \
         No prose, no markdown, no commentary — just the JSON.\n\n{}",
        serde_json::to_string_pretty(schema).unwrap_or_else(|_| "{}".into())
    )
}

fn read_thumb_as_part(path: &Path) -> Result<ChatPart> {
    let bytes = std::fs::read(path).with_context(|| format!("read thumb {}", path.display()))?;
    let b64 = BASE64.encode(&bytes);
    let url = format!("data:image/jpeg;base64,{}", b64);
    Ok(ChatPart::ImageUrl {
        image_url: ImageUrl { url },
    })
}

fn first_message_content(resp: &ChatResponse) -> Result<String> {
    let choice = resp
        .choices
        .first()
        .ok_or_else(|| anyhow!("local response has no choices"))?;
    Ok(choice.message.content.clone())
}

/// Strip surrounding ```json … ``` or ``` … ``` code fences if the
/// model wrapped its JSON in markdown, plus any leading/trailing
/// whitespace. Fences in the middle of the body are left alone — that
/// would be invalid JSON regardless and we want the parse error to
/// surface.
fn strip_json_fences(s: &str) -> String {
    let trimmed = s.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let stripped = stripped.trim_start_matches('\n');
    let stripped = stripped.strip_suffix("```").unwrap_or(stripped);
    stripped.trim().to_string()
}

/// Find the first balanced top-level `{...}` substring inside `s`.
/// Reasoning-mode models (Gemma 4, GPT-OSS, DeepSeek-R1) emit a long
/// thinking-trace preamble before the actual JSON answer; we need to
/// fish the JSON out of that. String-aware so braces inside string
/// literals don't fool the depth counter.
fn extract_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Some models, when given a tool schema named e.g. `record_shoot_summary`,
/// emit `{"record_shoot_summary": {...fields...}}` instead of bare
/// `{...fields...}`. Detect that one-level wrapping and unwrap.
fn maybe_unwrap_tool_call(json_str: &str, tool_name: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return json_str.to_string();
    };
    let Some(obj) = v.as_object() else {
        return json_str.to_string();
    };
    if obj.len() == 1 {
        if let Some(inner) = obj.get(tool_name) {
            if inner.is_object() {
                return inner.to_string();
            }
        }
    }
    json_str.to_string()
}

/// Walk every balanced `{...}` block in `s` and return the *last* one
/// whose top-level keys include `required_key`. Falls back to the first
/// balanced block if no match. This is a stronger version of
/// `extract_json_object` for reasoning models that emit multiple JSON
/// fragments (echoed schemas, partial drafts) before the final answer —
/// "last one with the right key" reliably picks the answer over the
/// schema dump.
fn extract_target_json<'a>(s: &'a str, required_key: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let mut best: Option<&str> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        let mut end = None;
        for j in i..bytes.len() {
            let b = bytes[j];
            if in_string {
                if escape {
                    escape = false;
                } else if b == b'\\' {
                    escape = true;
                } else if b == b'"' {
                    in_string = false;
                }
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(e) => {
                let block = &s[i..=e];
                if has_top_level_key(block, required_key) {
                    best = Some(block);
                }
                i = e + 1;
            }
            None => break,
        }
    }
    best.or_else(|| extract_json_object(s))
}

fn has_top_level_key(s: &str, key: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s)
        .ok()
        .and_then(|v| v.as_object().map(|o| o.contains_key(key)))
        .unwrap_or(false)
}

fn usage_from_resp(resp: &ChatResponse) -> Usage {
    match &resp.usage {
        Some(u) => Usage {
            input_tokens: u.prompt_tokens.unwrap_or(0),
            output_tokens: u.completion_tokens.unwrap_or(0),
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        },
        None => Usage::default(),
    }
}

async fn send_with_retries(
    http: &reqwest::Client,
    url: &str,
    headers: &HeaderMap,
    req: &ChatRequest<'_>,
) -> Result<ChatResponse> {
    let backoff_ms = [1000u64, 4000, 16000];
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..=backoff_ms.len() {
        let resp_result = http
            .post(url)
            .headers(headers.clone())
            .json(req)
            .send()
            .await;
        match resp_result {
            Ok(resp) => {
                let status = resp.status();
                if status == StatusCode::UNAUTHORIZED {
                    let body = resp.text().await.unwrap_or_default();
                    bail!("local 401: {}", body);
                }
                if status == StatusCode::OK {
                    let parsed: ChatResponse = resp
                        .json()
                        .await
                        .context("parse local /chat/completions response")?;
                    return Ok(parsed);
                }
                let body = resp.text().await.unwrap_or_default();
                let err = anyhow!("local {}: {}", status, body);
                let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                if !retryable || attempt == backoff_ms.len() {
                    return Err(err);
                }
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(backoff_ms[attempt])).await;
            }
            Err(e) => {
                if attempt == backoff_ms.len() {
                    return Err(anyhow!(e).context("local network error after retries"));
                }
                last_err = Some(anyhow!(e).context("local network error"));
                tokio::time::sleep(Duration::from_millis(backoff_ms[attempt])).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("retry loop exited unexpectedly")))
}

// ---- Wire types: OpenAI Chat Completions ----

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

/// One chat message. Either plain string content (for short text-only
/// system/user) or a typed parts array (when there are images).
#[derive(Serialize)]
#[serde(untagged)]
enum ChatMessage {
    Text { role: &'static str, content: String },
    Parts { role: &'static str, content: Vec<ChatPart> },
}

impl ChatMessage {
    fn system_text(s: &str) -> Self {
        ChatMessage::Text {
            role: "system",
            content: s.to_string(),
        }
    }
    fn user_text(s: &str) -> Self {
        ChatMessage::Text {
            role: "user",
            content: s.to_string(),
        }
    }
    fn user_parts(parts: Vec<ChatPart>) -> Self {
        ChatMessage::Parts {
            role: "user",
            content: parts,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChatPart {
    Text { text: String },
    ImageUrl {
        #[serde(rename = "image_url")]
        image_url: ImageUrl,
    },
}

impl ChatPart {
    fn text(s: impl Into<String>) -> Self {
        ChatPart::Text { text: s.into() }
    }
}

#[derive(Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(Deserialize, Debug)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize, Debug)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize, Debug)]
struct ChatResponseMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize, Debug)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(json_body: &str) -> ChatResponse {
        ChatResponse {
            choices: vec![ChatChoice {
                message: ChatResponseMessage {
                    content: json_body.to_string(),
                },
            }],
            usage: Some(ChatUsage {
                prompt_tokens: Some(1500),
                completion_tokens: Some(800),
            }),
        }
    }

    #[test]
    fn strips_json_fences() {
        assert_eq!(strip_json_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_json_fences("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_json_fences("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_json_fences("  ```json\n{\"a\":1}\n```  "), "{\"a\":1}");
    }

    #[test]
    fn parse_judgments_from_clean_json() {
        let resp = fixture(
            r#"{
                "judgments": [
                    {"photo_id":1,"composition":7,"aesthetic":6,"cluster_rank":1,"is_keeper":true,"suggested_flag":"pick","reason":"ok"},
                    {"photo_id":2,"composition":4,"aesthetic":5,"cluster_rank":2,"is_keeper":false,"suggested_flag":"reject","reason":"soft"}
                ]
            }"#,
        );
        let content = first_message_content(&resp).unwrap();
        let cleaned = strip_json_fences(&content);
        #[derive(Deserialize)]
        struct Wrap {
            judgments: Vec<CuratorJudgment>,
        }
        let wrap: Wrap = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(wrap.judgments.len(), 2);
        assert_eq!(wrap.judgments[1].suggested_flag.as_str(), "reject");
    }

    #[test]
    fn parse_summary_from_fenced_json() {
        let resp = fixture(
            "```json\n{\"shoot_type\":\"wedding\",\"subjects\":[\"bride\"],\"story\":\"x\",\"dominant_style\":\"warm\",\"watch_for\":[\"closed eyes\"]}\n```",
        );
        let content = first_message_content(&resp).unwrap();
        let cleaned = strip_json_fences(&content);
        let s: ShootSummary = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(s.shoot_type, "wedding");
    }

    #[test]
    fn parse_errors_on_garbage() {
        let resp = fixture("I cannot do that.");
        let content = first_message_content(&resp).unwrap();
        let cleaned = strip_json_fences(&content);
        let res: Result<ShootSummary, _> = serde_json::from_str(&cleaned);
        assert!(res.is_err());
    }

    #[test]
    fn usage_maps_through() {
        let resp = fixture(r#"{"a":1}"#);
        let u = usage_from_resp(&resp);
        assert_eq!(u.input_tokens, 1500);
        assert_eq!(u.output_tokens, 800);
        assert_eq!(u.cache_read_input_tokens, 0);
    }

    #[test]
    fn empty_model_rejected() {
        let r = LocalProvider::new("http://localhost:11434/v1".into(), "".into(), None);
        assert!(r.is_err());
    }

    #[test]
    fn extract_json_skips_reasoning_preamble() {
        // Gemma 4-style channel preamble + final JSON.
        let s = "<|channel>thought\nThe user wants me to analyze...\n<channel|>```json\n{\"shoot_type\":\"x\",\"subjects\":[\"a\"],\"story\":\"y\",\"dominant_style\":\"z\",\"watch_for\":[\"w\"]}\n```";
        let stripped = strip_json_fences(s);
        let json = extract_json_object(&stripped).unwrap();
        let parsed: ShootSummary = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.shoot_type, "x");
    }

    #[test]
    fn extract_json_handles_strings_with_braces() {
        // Brace inside a string literal mustn't fool the depth counter.
        let s = r#"prefix {"reason": "looks like {something}", "ok": true} trailing"#;
        let json = extract_json_object(s).unwrap();
        assert_eq!(json, r#"{"reason": "looks like {something}", "ok": true}"#);
    }

    #[test]
    fn unwrap_tool_call_when_wrapped() {
        let wrapped = r#"{"record_shoot_summary": {"shoot_type":"x","subjects":[],"story":"","dominant_style":"","watch_for":[]}}"#;
        let unwrapped = maybe_unwrap_tool_call(wrapped, "record_shoot_summary");
        let parsed: ShootSummary = serde_json::from_str(&unwrapped).unwrap();
        assert_eq!(parsed.shoot_type, "x");
    }

    #[test]
    fn unwrap_tool_call_passthrough_when_not_wrapped() {
        let bare = r#"{"shoot_type":"x","subjects":[],"story":"","dominant_style":"","watch_for":[]}"#;
        let same = maybe_unwrap_tool_call(bare, "record_shoot_summary");
        let parsed: ShootSummary = serde_json::from_str(&same).unwrap();
        assert_eq!(parsed.shoot_type, "x");
    }
}
