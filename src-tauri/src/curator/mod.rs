//! AI Curator — calls a vision-LLM provider (Anthropic, Gemini, or a
//! local OpenAI-compatible server) to add compositional / aesthetic
//! judgments on top of the local AI's technical signals. Two-stage flow:
//! Stage 1 characterizes the shoot, Stage 2 evaluates each phash cluster
//! (and singletons in batches) using the Stage 1 summary as context.
//!
//! Sibling to `ai/`. The two subsystems run independently — an outage
//! in any LLM provider never stalls local face detection.

pub mod api_anthropic;
pub mod api_gemini;
pub mod api_local;
pub mod cost;
pub mod prompts;
pub mod provider;
pub mod types;
pub mod worker;

use serde::Serialize;

/// Operational status of the curator subsystem, reported back to the
/// frontend so the Library card can show "Idle / Running / Failed".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CuratorStatus {
    Idle,
    Running,
    Failed,
    Disabled,
}

/// The OS keychain service name. Debug builds use a `-dev` suffix so dev
/// work doesn't read or overwrite production curator keys. Account name
/// varies per provider — see `keyring_account_for`.
#[cfg(debug_assertions)]
pub const KEYRING_SERVICE: &str = "photosift-dev";
#[cfg(not(debug_assertions))]
pub const KEYRING_SERVICE: &str = "photosift";

/// Per-provider keychain account names. `"local"` providers don't use
/// a key by default but the slot is reserved if a user ever points us
/// at a remote OpenAI-compatible service that wants Bearer auth.
pub fn keyring_account_for(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "anthropic_api_key",
        "gemini" => "gemini_api_key",
        "local" => "local_api_key",
        // Fallback shouldn't fire in practice (UI is enum-typed), but
        // returning a distinct string keeps lookups deterministic.
        _ => "unknown_provider_api_key",
    }
}

/// Default model string per provider. Settings persist a per-provider
/// model so flipping the dropdown remembers the user's last choice.
pub fn default_model_for(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-sonnet-4-6",
        "gemini" => "gemini-2.5-flash",
        "local" => "",
        _ => "claude-sonnet-4-6",
    }
}

/// Pretty name used in user-facing strings (import dialog, badges).
#[allow(dead_code)]
pub fn provider_display_name(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "Anthropic",
        "gemini" => "Gemini",
        "local" => "Local",
        _ => "Unknown",
    }
}

/// Default model string for the legacy single-model setting. Existing
/// DBs are upgraded so the per-provider Anthropic model defaults to
/// this when the migration runs.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

pub use worker::{spawn as spawn_worker, CuratorJob, ProgressCounters, WorkerHandle};
