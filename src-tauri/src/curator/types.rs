//! Wire types for the Anthropic Messages API and the persisted curator
//! domain model. Kept narrow on purpose: only the fields we actually
//! send or read. Anthropic returns extra fields we don't care about,
//! so most response structs use `serde(deny_unknown_fields = false)`
//! implicitly via not opting in.

use serde::{Deserialize, Serialize};

// ---- Persisted domain types ----

/// One Claude judgment for one photo. Mirrors a `curator_judgments` row.
/// Lives in `curator/types.rs` so the API client and the DB layer share
/// one definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CuratorJudgment {
    pub photo_id: i64,
    /// 0-10. Framing, balance, leading lines, subject placement.
    pub composition: i32,
    /// 0-10. Light, color, mood, overall keeper-quality.
    pub aesthetic: i32,
    /// 1-based rank within this cluster, 1=best. None for singletons.
    pub cluster_rank: Option<i32>,
    /// Whether Claude considers this a keeper standalone (not just
    /// best-of-cluster). For singletons this is the only useful field.
    pub is_keeper: bool,
    /// User-action recommendation. Lowercase string so the JSON contract
    /// matches what the model is asked to emit.
    pub suggested_flag: SuggestedFlag,
    /// One-sentence rationale, surfaced verbatim in the Triage chip.
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestedFlag {
    Pick,
    Reject,
    Keep,
}

impl SuggestedFlag {
    pub fn as_str(self) -> &'static str {
        match self {
            SuggestedFlag::Pick => "pick",
            SuggestedFlag::Reject => "reject",
            SuggestedFlag::Keep => "keep",
        }
    }
}

/// Stage 1 output: the shoot characterization persisted verbatim into
/// `shoots.curator_summary`. Inlined into Stage 2 system prompts so the
/// per-cluster judgments share context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShootSummary {
    pub shoot_type: String,
    pub subjects: Vec<String>,
    pub story: String,
    pub dominant_style: String,
    pub watch_for: Vec<String>,
}

// ---- Anthropic Messages API request/response wire types ----

#[derive(Debug, Serialize)]
pub struct MessagesRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    /// System prompt as a *content block array* so we can mark blocks
    /// with `cache_control`. Anthropic accepts both a plain string and
    /// an array; we always use the array form.
    pub system: Vec<SystemBlock<'a>>,
    pub messages: Vec<Message<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice<'a>>,
}

#[derive(Debug, Serialize)]
pub struct SystemBlock<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str, // "text"
    pub text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Serialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: &'static str, // "ephemeral"
}

impl CacheControl {
    pub fn ephemeral() -> Self {
        Self { kind: "ephemeral" }
    }
}

#[derive(Debug, Serialize)]
pub struct Message<'a> {
    pub role: &'a str, // "user" | "assistant"
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ContentBlock {
    Text {
        #[serde(rename = "type")]
        kind: &'static str, // "text"
        text: String,
    },
    Image {
        #[serde(rename = "type")]
        kind: &'static str, // "image"
        source: ImageSource,
    },
}

impl ContentBlock {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text { kind: "text", text: s.into() }
    }
    pub fn image_b64(media_type: &'static str, data: String) -> Self {
        Self::Image {
            kind: "image",
            source: ImageSource {
                kind: "base64",
                media_type,
                data,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub kind: &'static str, // "base64"
    pub media_type: &'static str, // "image/jpeg"
    pub data: String,
}

#[derive(Debug, Serialize)]
pub struct Tool<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice<'a> {
    Tool { name: &'a str },
}

// ---- Response side ----

#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub model: String,
    pub content: Vec<ResponseContent>,
    pub usage: Usage,
    #[allow(dead_code)]
    #[serde(default)]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseContent {
    Text {
        text: String,
    },
    ToolUse {
        #[allow(dead_code)]
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Usage block. Anthropic's response includes `cache_creation_input_tokens`
/// and `cache_read_input_tokens` when prompt caching is in play; both are
/// reported separately from `input_tokens`. Cost computation in `cost.rs`
/// uses all three.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

// ---- Schema for the `record_judgments` tool ----

/// The JSON-schema body sent in `tools[0].input_schema`. Built once,
/// reused across every Stage 2 call.
pub fn judgments_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "judgments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "photo_id":    { "type": "integer" },
                        "composition": { "type": "integer", "minimum": 0, "maximum": 10 },
                        "aesthetic":   { "type": "integer", "minimum": 0, "maximum": 10 },
                        "cluster_rank":{ "type": ["integer", "null"], "minimum": 1 },
                        "is_keeper":   { "type": "boolean" },
                        "suggested_flag": {
                            "type": "string",
                            "enum": ["pick", "reject", "keep"]
                        },
                        "reason":      { "type": "string", "maxLength": 220 }
                    },
                    "required": [
                        "photo_id", "composition", "aesthetic",
                        "is_keeper", "suggested_flag", "reason"
                    ]
                }
            }
        },
        "required": ["judgments"]
    })
}

/// Schema for the Stage 1 `record_shoot_summary` tool.
pub fn summary_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "shoot_type":     { "type": "string" },
            "subjects":       { "type": "array", "items": { "type": "string" } },
            "story":          { "type": "string" },
            "dominant_style": { "type": "string" },
            "watch_for":      { "type": "array", "items": { "type": "string" } },
        },
        "required": ["shoot_type", "subjects", "story", "dominant_style", "watch_for"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judgment_round_trips_through_json() {
        let j = CuratorJudgment {
            photo_id: 4271,
            composition: 7,
            aesthetic: 6,
            cluster_rank: Some(1),
            is_keeper: true,
            suggested_flag: SuggestedFlag::Pick,
            reason: "Strongest framing of the burst.".to_string(),
        };
        let s = serde_json::to_string(&j).unwrap();
        let back: CuratorJudgment = serde_json::from_str(&s).unwrap();
        assert_eq!(back.photo_id, 4271);
        assert_eq!(back.suggested_flag.as_str(), "pick");
    }

    #[test]
    fn singleton_judgment_has_null_cluster_rank() {
        let s = r#"{
            "photo_id": 99,
            "composition": 5,
            "aesthetic": 4,
            "cluster_rank": null,
            "is_keeper": false,
            "suggested_flag": "reject",
            "reason": "Soft focus on subject."
        }"#;
        let j: CuratorJudgment = serde_json::from_str(s).unwrap();
        assert!(j.cluster_rank.is_none());
        assert_eq!(j.suggested_flag, SuggestedFlag::Reject);
    }
}
