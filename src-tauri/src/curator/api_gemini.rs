//! Google Gemini provider. Talks to
//! `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`.
//!
//! Reuses the same Stage 1 / Stage 2 prompts and JSON Schema tool
//! definitions as the Anthropic path — Gemini's `functionDeclarations`
//! accepts the same OpenAPI-subset schema. Differences from Anthropic
//! that necessitate a separate impl:
//! - Auth is a `?key=` query param, not a header.
//! - Wire field names are camelCase (`systemInstruction`, `inlineData`,
//!   `functionCall`, `usageMetadata`).
//! - No per-request prompt cache control. Gemini has its own
//!   `cachedContents` API but that's separate work; v1 omits it.

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
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const SUMMARY_TOOL_NAME: &str = "record_shoot_summary";
const JUDGMENTS_TOOL_NAME: &str = "record_judgments";
const PROVIDER_ID: &str = "gemini";

#[derive(Clone)]
pub struct GeminiProvider {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("build reqwest client")?;
        Ok(Self { http, api_key, model })
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/{}:generateContent?key={}",
            API_BASE, self.model, self.api_key
        )
    }
}

#[async_trait]
impl CuratorProvider for GeminiProvider {
    async fn test_connection(&self) -> Result<()> {
        let req = GeminiRequest {
            system_instruction: None,
            contents: vec![Content {
                role: "user",
                parts: vec![Part::Text { text: "ping".into() }],
            }],
            tools: None,
            tool_config: None,
            generation_config: Some(GenerationConfig {
                max_output_tokens: Some(8),
                response_mime_type: None,
            }),
        };
        let resp = self
            .http
            .post(self.endpoint())
            .json(&req)
            .send()
            .await
            .context("network error")?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            bail!("API key rejected ({})", status);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("non-2xx from Gemini: {} {}", status, body);
        }
        Ok(())
    }

    async fn run_stage1(
        &self,
        photo_count: i64,
        cluster_count: i64,
        thumb_paths: &[&Path],
    ) -> Result<SummaryResult> {
        let mut parts: Vec<Part> = Vec::with_capacity(thumb_paths.len() + 1);
        parts.push(Part::Text {
            text: stage1_user_text(photo_count, cluster_count),
        });
        for p in thumb_paths {
            parts.push(read_thumb_as_part(p)?);
        }

        let req = GeminiRequest {
            system_instruction: Some(Content {
                role: "system",
                parts: vec![Part::Text { text: stage1_system() }],
            }),
            contents: vec![Content { role: "user", parts }],
            tools: Some(vec![ToolDecl {
                function_declarations: vec![FunctionDecl {
                    name: SUMMARY_TOOL_NAME.into(),
                    description: "Persist the shoot characterization.".into(),
                    parameters: summary_tool_schema(),
                }],
            }]),
            tool_config: Some(ToolConfig {
                function_calling_config: FunctionCallingConfig {
                    mode: "ANY".into(),
                    allowed_function_names: vec![SUMMARY_TOOL_NAME.into()],
                },
            }),
            generation_config: Some(GenerationConfig {
                max_output_tokens: Some(600),
                response_mime_type: None,
            }),
        };

        let resp = send_with_retries(&self.http, &self.endpoint(), &req).await?;
        let summary = parse_function_call::<ShootSummary>(&resp, SUMMARY_TOOL_NAME)?;
        Ok(SummaryResult {
            summary,
            usage: usage_from_metadata(&resp.usage_metadata),
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

        let mut parts: Vec<Part> = Vec::with_capacity(cluster.frames.len() + 1);
        parts.push(Part::Text {
            text: stage2_user_text(cluster.frames.len(), &tech_lines),
        });
        for f in cluster.frames {
            parts.push(read_thumb_as_part(f.thumb_path)?);
        }

        let req = GeminiRequest {
            system_instruction: Some(Content {
                role: "system",
                parts: vec![Part::Text { text: stage2_system(summary) }],
            }),
            contents: vec![Content { role: "user", parts }],
            tools: Some(vec![ToolDecl {
                function_declarations: vec![FunctionDecl {
                    name: JUDGMENTS_TOOL_NAME.into(),
                    description: "Persist per-frame judgments for this cluster.".into(),
                    parameters: judgments_tool_schema(),
                }],
            }]),
            tool_config: Some(ToolConfig {
                function_calling_config: FunctionCallingConfig {
                    mode: "ANY".into(),
                    allowed_function_names: vec![JUDGMENTS_TOOL_NAME.into()],
                },
            }),
            generation_config: Some(GenerationConfig {
                max_output_tokens: Some(220 * cluster.frames.len() as u32),
                response_mime_type: None,
            }),
        };

        let resp = send_with_retries(&self.http, &self.endpoint(), &req).await?;
        #[derive(Deserialize)]
        struct Wrap {
            judgments: Vec<CuratorJudgment>,
        }
        let wrap = parse_function_call::<Wrap>(&resp, JUDGMENTS_TOOL_NAME)?;
        Ok(ClusterJudgmentBatch {
            judgments: wrap.judgments,
            usage: usage_from_metadata(&resp.usage_metadata),
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

fn read_thumb_as_part(path: &Path) -> Result<Part> {
    let bytes = std::fs::read(path).with_context(|| format!("read thumb {}", path.display()))?;
    let b64 = BASE64.encode(&bytes);
    Ok(Part::InlineData {
        inline_data: InlineData {
            mime_type: "image/jpeg".into(),
            data: b64,
        },
    })
}

fn usage_from_metadata(m: &Option<UsageMetadata>) -> Usage {
    match m {
        Some(u) => Usage {
            input_tokens: u.prompt_token_count.unwrap_or(0),
            output_tokens: u.candidates_token_count.unwrap_or(0),
            cache_read_input_tokens: u.cached_content_token_count.unwrap_or(0),
            cache_creation_input_tokens: 0,
        },
        None => Usage::default(),
    }
}

async fn send_with_retries(
    http: &reqwest::Client,
    url: &str,
    req: &GeminiRequest<'_>,
) -> Result<GeminiResponse> {
    let backoff_ms = [1000u64, 4000, 16000];
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..=backoff_ms.len() {
        let resp_result = http.post(url).json(req).send().await;
        match resp_result {
            Ok(resp) => {
                let status = resp.status();
                if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                    let body = resp.text().await.unwrap_or_default();
                    bail!("Gemini auth rejected: {} {}", status, body);
                }
                if status == StatusCode::OK {
                    let parsed: GeminiResponse = resp
                        .json()
                        .await
                        .context("parse Gemini response body")?;
                    return Ok(parsed);
                }
                let retry_after_secs = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());
                let body = resp.text().await.unwrap_or_default();
                let err = anyhow!("gemini {}: {}", status, body);

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
                    return Err(anyhow!(e).context("gemini network error after retries"));
                }
                last_err = Some(anyhow!(e).context("gemini network error"));
                tokio::time::sleep(Duration::from_millis(backoff_ms[attempt])).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("retry loop exited unexpectedly")))
}

/// Pull the named functionCall from the first candidate and decode its
/// args into `T`. Tolerates extra parts (text + functionCall mixed).
fn parse_function_call<T: serde::de::DeserializeOwned>(
    resp: &GeminiResponse,
    tool_name: &str,
) -> Result<T> {
    let cand = resp
        .candidates
        .first()
        .ok_or_else(|| anyhow!("Gemini response has no candidates"))?;
    for part in &cand.content.parts {
        if let ResponsePart::FunctionCall { function_call } = part {
            if function_call.name == tool_name {
                return serde_json::from_value(function_call.args.clone())
                    .with_context(|| format!("decode {} args", tool_name));
            }
        }
    }
    bail!("Gemini response did not include a {} functionCall", tool_name)
}

// ---- Wire types ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content<'a>>,
    contents: Vec<Content<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDecl>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<ToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize)]
struct Content<'a> {
    role: &'a str,
    parts: Vec<Part>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Part {
    Text { text: String },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: InlineData,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolDecl {
    function_declarations: Vec<FunctionDecl>,
}

#[derive(Serialize)]
struct FunctionDecl {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolConfig {
    function_calling_config: FunctionCallingConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FunctionCallingConfig {
    mode: String,
    allowed_function_names: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(default)]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Deserialize, Debug)]
struct Candidate {
    content: ResponseContent,
}

#[derive(Deserialize, Debug)]
struct ResponseContent {
    parts: Vec<ResponsePart>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum ResponsePart {
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
    },
    // Text-only parts get matched here as a catch-all so they don't
    // break parsing when the model returns a mix of text + tool call.
    Other(serde_json::Value),
}

#[derive(Deserialize, Debug)]
struct FunctionCall {
    name: String,
    #[serde(default)]
    args: serde_json::Value,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    #[serde(default)]
    prompt_token_count: Option<u32>,
    #[serde(default)]
    candidates_token_count: Option<u32>,
    #[serde(default)]
    cached_content_token_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(args: serde_json::Value, tool_name: &str) -> GeminiResponse {
        GeminiResponse {
            candidates: vec![Candidate {
                content: ResponseContent {
                    parts: vec![ResponsePart::FunctionCall {
                        function_call: FunctionCall {
                            name: tool_name.to_string(),
                            args,
                        },
                    }],
                },
            }],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: Some(1500),
                candidates_token_count: Some(800),
                cached_content_token_count: None,
            }),
        }
    }

    #[test]
    fn parse_judgments_extracts_array() {
        #[derive(Deserialize)]
        struct Wrap {
            judgments: Vec<CuratorJudgment>,
        }
        let resp = fixture(
            serde_json::json!({
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
            JUDGMENTS_TOOL_NAME,
        );
        let wrap = parse_function_call::<Wrap>(&resp, JUDGMENTS_TOOL_NAME).unwrap();
        assert_eq!(wrap.judgments.len(), 2);
        assert_eq!(wrap.judgments[0].photo_id, 1);
        assert_eq!(wrap.judgments[1].suggested_flag.as_str(), "reject");
    }

    #[test]
    fn parse_summary_extracts_struct() {
        let resp = fixture(
            serde_json::json!({
                "shoot_type": "wedding",
                "subjects": ["bride", "groom"],
                "story": "ceremony at golden hour",
                "dominant_style": "warm naturalistic",
                "watch_for": ["closed eyes", "occluded faces"]
            }),
            SUMMARY_TOOL_NAME,
        );
        let s = parse_function_call::<ShootSummary>(&resp, SUMMARY_TOOL_NAME).unwrap();
        assert_eq!(s.shoot_type, "wedding");
        assert_eq!(s.subjects.len(), 2);
    }

    #[test]
    fn parse_errors_when_no_function_call() {
        // Candidate exists, but parts contain only a text response.
        let resp = GeminiResponse {
            candidates: vec![Candidate {
                content: ResponseContent {
                    parts: vec![ResponsePart::Other(serde_json::json!({
                        "text": "I cannot do that."
                    }))],
                },
            }],
            usage_metadata: None,
        };
        assert!(parse_function_call::<ShootSummary>(&resp, SUMMARY_TOOL_NAME).is_err());
    }

    #[test]
    fn usage_metadata_maps_to_usage() {
        let m = Some(UsageMetadata {
            prompt_token_count: Some(2000),
            candidates_token_count: Some(500),
            cached_content_token_count: Some(300),
        });
        let u = usage_from_metadata(&m);
        assert_eq!(u.input_tokens, 2000);
        assert_eq!(u.output_tokens, 500);
        assert_eq!(u.cache_read_input_tokens, 300);
        assert_eq!(u.cache_creation_input_tokens, 0);
    }
}
