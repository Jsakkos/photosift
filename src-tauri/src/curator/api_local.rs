//! Local OpenAI-compatible provider. Talks to any server that exposes
//! `POST {base_url}/chat/completions` (Ollama, LM Studio, vLLM,
//! llama.cpp server, …).
//!
//! Differences vs. the cloud providers:
//! - No tool calling — many small vision models flake on `tool_calls`,
//!   so we use `response_format: { "type": "json_object" }` plus a
//!   schema embedded in the system prompt and parse the JSON content.
//! - Markdown code fences (```json … ```) are stripped before parsing.
//!   Ollama with default settings sometimes wraps JSON in fences even
//!   when JSON-mode is requested.
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
            response_format: None,
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
            response_format: Some(ResponseFormat { kind: "json_object" }),
            max_tokens: Some(600),
            stream: false,
        };

        let resp = send_with_retries(&self.http, &self.chat_url(), &self.auth_headers()?, &req)
            .await?;
        let content = first_message_content(&resp)?;
        let cleaned = strip_json_fences(&content);
        let summary: ShootSummary =
            serde_json::from_str(&cleaned).context("decode local Stage 1 JSON")?;
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
            response_format: Some(ResponseFormat { kind: "json_object" }),
            max_tokens: Some(220 * cluster.frames.len() as u32),
            stream: false,
        };

        let resp = send_with_retries(&self.http, &self.chat_url(), &self.auth_headers()?, &req)
            .await?;
        let content = first_message_content(&resp)?;
        let cleaned = strip_json_fences(&content);
        #[derive(Deserialize)]
        struct Wrap {
            judgments: Vec<CuratorJudgment>,
        }
        let wrap: Wrap = serde_json::from_str(&cleaned).context("decode local Stage 2 JSON")?;
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
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
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
}
