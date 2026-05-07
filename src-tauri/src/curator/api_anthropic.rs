//! Anthropic Messages API client. Constructs Stage 1 (shoot summary)
//! and Stage 2 (per-cluster judgment) requests, fires them via reqwest
//! with a small retry/backoff policy, and parses the tool-use response
//! into typed domain values.

use crate::curator::prompts::{
    stage1_system, stage1_user_text, stage2_system, stage2_user_text, CURRENT_PROMPT_VERSION,
};
use crate::curator::provider::{
    ClusterJudgmentBatch, CuratorProvider, Stage2Cluster, SummaryResult,
};
use crate::curator::types::{
    judgments_tool_schema, summary_tool_schema, CacheControl, ContentBlock, CuratorJudgment,
    Message, MessagesRequest, MessagesResponse, ResponseContent, ShootSummary, SystemBlock, Tool,
    ToolChoice,
};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::StatusCode;
use std::path::Path;
use std::time::Duration;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const PROMPT_CACHING_BETA: &str = "prompt-caching-2024-07-31";
const SUMMARY_TOOL_NAME: &str = "record_shoot_summary";
const JUDGMENTS_TOOL_NAME: &str = "record_judgments";
const PROVIDER_ID: &str = "anthropic";

/// Holds the API key + reqwest client, parameterised by model. One
/// instance per worker (or per shoot run).
#[derive(Clone)]
pub struct AnthropicProvider {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("build reqwest client")?;
        Ok(Self { http, api_key, model })
    }

    fn auth_headers(&self) -> Result<HeaderMap> {
        let mut h = HeaderMap::new();
        h.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key).context("api key contains invalid bytes")?,
        );
        h.insert("anthropic-version", HeaderValue::from_static(ANTHROPIC_VERSION));
        h.insert("anthropic-beta", HeaderValue::from_static(PROMPT_CACHING_BETA));
        h.insert("content-type", HeaderValue::from_static("application/json"));
        Ok(h)
    }
}

#[async_trait]
impl CuratorProvider for AnthropicProvider {
    /// Cheap key-validity probe used by the Settings "Test connection"
    /// button. Sends a single-token text message; if it returns 200 the
    /// key is good. Distinguishes 401 from network/other errors.
    async fn test_connection(&self) -> Result<()> {
        let req = MessagesRequest {
            model: &self.model,
            max_tokens: 8,
            system: vec![],
            messages: vec![Message {
                role: "user",
                content: vec![ContentBlock::text("ping")],
            }],
            tools: None,
            tool_choice: None,
        };
        let resp = self
            .http
            .post(API_URL)
            .headers(self.auth_headers()?)
            .json(&req)
            .send()
            .await
            .context("network error")?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            bail!("API key rejected (401)");
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("non-2xx from /messages: {} {}", status, body);
        }
        Ok(())
    }

    async fn run_stage1(
        &self,
        photo_count: i64,
        cluster_count: i64,
        thumb_paths: &[&Path],
    ) -> Result<SummaryResult> {
        let images = thumb_paths
            .iter()
            .map(|p| read_thumb_as_block(p))
            .collect::<Result<Vec<_>>>()?;
        let mut content: Vec<ContentBlock> = Vec::with_capacity(images.len() + 1);
        content.push(ContentBlock::text(stage1_user_text(photo_count, cluster_count)));
        content.extend(images);

        let system_text = stage1_system();
        let tool_schema = summary_tool_schema();
        let req = MessagesRequest {
            model: &self.model,
            max_tokens: 600,
            system: vec![SystemBlock {
                kind: "text",
                text: &system_text,
                cache_control: None,
            }],
            messages: vec![Message { role: "user", content }],
            tools: Some(vec![Tool {
                name: SUMMARY_TOOL_NAME,
                description: "Persist the shoot characterization.",
                input_schema: tool_schema,
            }]),
            tool_choice: Some(ToolChoice::Tool { name: SUMMARY_TOOL_NAME }),
        };

        let resp = send_with_retries(&self.http, &self.auth_headers()?, &req).await?;
        let summary = parse_tool_use_summary(&resp)?;
        Ok(SummaryResult {
            summary,
            usage: resp.usage,
            model: self.model.clone(),
        })
    }

    async fn run_stage2_cluster(
        &self,
        summary: &ShootSummary,
        cluster: &Stage2Cluster<'_>,
    ) -> Result<ClusterJudgmentBatch> {
        let mut content: Vec<ContentBlock> = Vec::with_capacity(cluster.frames.len() + 1);
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
        content.push(ContentBlock::text(stage2_user_text(cluster.frames.len(), &tech_lines)));
        for f in cluster.frames {
            content.push(read_thumb_as_block(f.thumb_path)?);
        }

        let system_text = stage2_system(summary);
        let req = MessagesRequest {
            model: &self.model,
            max_tokens: 220 * cluster.frames.len() as u32,
            // The Stage 2 system prompt is the cached prefix. We mark it
            // ephemeral so subsequent Stage 2 calls in the same shoot
            // hit the prompt cache (5-min TTL).
            system: vec![SystemBlock {
                kind: "text",
                text: &system_text,
                cache_control: Some(CacheControl::ephemeral()),
            }],
            messages: vec![Message { role: "user", content }],
            tools: Some(vec![Tool {
                name: JUDGMENTS_TOOL_NAME,
                description: "Persist per-frame judgments for this cluster.",
                input_schema: judgments_tool_schema(),
            }]),
            tool_choice: Some(ToolChoice::Tool { name: JUDGMENTS_TOOL_NAME }),
        };

        let resp = send_with_retries(&self.http, &self.auth_headers()?, &req).await?;
        let judgments = parse_tool_use_judgments(&resp)?;
        Ok(ClusterJudgmentBatch {
            judgments,
            usage: resp.usage,
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

fn read_thumb_as_block(path: &Path) -> Result<ContentBlock> {
    let bytes = std::fs::read(path).with_context(|| format!("read thumb {}", path.display()))?;
    let b64 = BASE64.encode(&bytes);
    Ok(ContentBlock::image_b64("image/jpeg", b64))
}

/// HTTP retry: on 429/5xx + retry-after, exponential backoff (1s, 4s, 16s).
async fn send_with_retries(
    http: &reqwest::Client,
    headers: &HeaderMap,
    req: &MessagesRequest<'_>,
) -> Result<MessagesResponse> {
    let backoff_ms = [1000u64, 4000, 16000];
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..=backoff_ms.len() {
        let resp_result = http
            .post(API_URL)
            .headers(headers.clone())
            .json(req)
            .send()
            .await;

        match resp_result {
            Ok(resp) => {
                let status = resp.status();
                if status == StatusCode::UNAUTHORIZED {
                    // Don't retry auth errors.
                    let body = resp.text().await.unwrap_or_default();
                    bail!("401 Unauthorized: {}", body);
                }
                if status == StatusCode::OK {
                    let parsed: MessagesResponse = resp
                        .json()
                        .await
                        .context("parse Anthropic response body")?;
                    return Ok(parsed);
                }
                let retry_after_secs = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());
                let body = resp.text().await.unwrap_or_default();
                let err = anyhow!("anthropic {}: {}", status, body);

                let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                if !retryable || attempt == backoff_ms.len() {
                    return Err(err);
                }
                let wait_ms = match retry_after_secs {
                    Some(s) => s.saturating_mul(1000),
                    None => backoff_ms[attempt],
                };
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }
            Err(e) => {
                if attempt == backoff_ms.len() {
                    return Err(anyhow!(e).context("anthropic network error after retries"));
                }
                last_err = Some(anyhow!(e).context("anthropic network error"));
                tokio::time::sleep(Duration::from_millis(backoff_ms[attempt])).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("retry loop exited unexpectedly")))
}

fn parse_tool_use_summary(resp: &MessagesResponse) -> Result<ShootSummary> {
    for block in &resp.content {
        if let ResponseContent::ToolUse { name, input, .. } = block {
            if name == SUMMARY_TOOL_NAME {
                return serde_json::from_value(input.clone())
                    .context("decode shoot summary tool input");
            }
        }
    }
    bail!("Stage 1 response did not include a {} tool_use", SUMMARY_TOOL_NAME)
}

fn parse_tool_use_judgments(resp: &MessagesResponse) -> Result<Vec<CuratorJudgment>> {
    for block in &resp.content {
        if let ResponseContent::ToolUse { name, input, .. } = block {
            if name == JUDGMENTS_TOOL_NAME {
                #[derive(serde::Deserialize)]
                struct Wrap {
                    judgments: Vec<CuratorJudgment>,
                }
                let w: Wrap = serde_json::from_value(input.clone())
                    .context("decode judgments tool input")?;
                return Ok(w.judgments);
            }
        }
    }
    bail!(
        "Stage 2 response did not include a {} tool_use",
        JUDGMENTS_TOOL_NAME
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curator::types::{ResponseContent, Usage};

    #[test]
    fn parse_judgments_extracts_array() {
        let resp = MessagesResponse {
            id: "msg_1".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            stop_reason: Some("tool_use".to_string()),
            usage: Usage::default(),
            content: vec![ResponseContent::ToolUse {
                id: "tool_1".to_string(),
                name: JUDGMENTS_TOOL_NAME.to_string(),
                input: serde_json::json!({
                    "judgments": [
                        {
                            "photo_id": 1, "composition": 7, "aesthetic": 6,
                            "cluster_rank": 1, "is_keeper": true,
                            "suggested_flag": "pick", "reason": "ok"
                        },
                        {
                            "photo_id": 2, "composition": 4, "aesthetic": 5,
                            "cluster_rank": 2, "is_keeper": false,
                            "suggested_flag": "reject", "reason": "soft"
                        }
                    ]
                }),
            }],
        };
        let judgments = parse_tool_use_judgments(&resp).unwrap();
        assert_eq!(judgments.len(), 2);
        assert_eq!(judgments[0].photo_id, 1);
        assert_eq!(judgments[1].suggested_flag.as_str(), "reject");
    }

    #[test]
    fn parse_judgments_errors_when_no_tool_use() {
        let resp = MessagesResponse {
            id: "msg_1".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            stop_reason: Some("end_turn".to_string()),
            usage: Usage::default(),
            content: vec![ResponseContent::Text {
                text: "I cannot do that.".to_string(),
            }],
        };
        assert!(parse_tool_use_judgments(&resp).is_err());
    }

    /// Live integration test, gated by env var so CI doesn't burn API
    /// budget. Run with: `CURATOR_TEST_KEY=sk-ant-... cargo test --
    /// curator::api_anthropic::tests::live_test_connection --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn live_test_connection() {
        let key = match std::env::var("CURATOR_TEST_KEY") {
            Ok(k) => k,
            Err(_) => return,
        };
        let provider = AnthropicProvider::new(key, "claude-haiku-4-5".to_string()).unwrap();
        provider.test_connection().await.unwrap();
    }
}
