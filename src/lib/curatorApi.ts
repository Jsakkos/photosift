// Typed wrappers around the curator Tauri commands. Centralizing here
// keeps the stringly-typed `invoke` calls out of components.

import { invoke } from "@tauri-apps/api/core";
import type {
  AnthropicApiKeyStatus,
  ApiKeyStatus,
  CuratorAgreementStats,
  CuratorJudgment,
  CuratorProvider,
  CuratorRunStatus,
  CuratorShootSummary,
} from "../types";

// ---- Generic per-provider API key management ----

export async function setCuratorApiKey(
  provider: CuratorProvider,
  apiKey: string,
): Promise<void> {
  await invoke("set_curator_api_key", { provider, apiKey });
}

export async function clearCuratorApiKey(provider: CuratorProvider): Promise<void> {
  await invoke("clear_curator_api_key", { provider });
}

export async function getCuratorApiKeyStatus(
  provider: CuratorProvider,
): Promise<ApiKeyStatus> {
  return await invoke<ApiKeyStatus>("get_curator_api_key_status", { provider });
}

/// Test the connection for the currently-selected provider (read from
/// settings on the Rust side). Local provider needs no key but still
/// probes `/v1/models` to confirm the server is reachable.
export async function testCuratorConnection(): Promise<void> {
  await invoke("test_curator_connection");
}

// ---- Backwards-compat Anthropic-named wrappers ----
//
// Older code paths still call these. They forward to the generic
// commands; remove on a follow-up sweep.

export async function setAnthropicApiKey(apiKey: string): Promise<void> {
  await invoke("set_anthropic_api_key", { apiKey });
}

export async function clearAnthropicApiKey(): Promise<void> {
  await invoke("clear_anthropic_api_key");
}

export async function getAnthropicApiKeyStatus(): Promise<AnthropicApiKeyStatus> {
  return await invoke<AnthropicApiKeyStatus>("get_anthropic_api_key_status");
}

export async function testAnthropicConnection(): Promise<void> {
  await invoke("test_anthropic_connection");
}

// ---- Run control ----

export async function startCuratorForShoot(shootId: number): Promise<void> {
  await invoke("start_curator_for_shoot", { shootId });
}

export async function cancelCurator(): Promise<void> {
  await invoke("cancel_curator");
}

export async function resumeCuratorForShoot(shootId: number): Promise<void> {
  await invoke("resume_curator_for_shoot", { shootId });
}

export async function clearCuratorForShoot(shootId: number): Promise<void> {
  await invoke("clear_curator_for_shoot", { shootId });
}

// ---- Read accessors ----

export async function getCuratorStatus(): Promise<CuratorRunStatus> {
  return await invoke<CuratorRunStatus>("get_curator_status");
}

export async function getCuratorJudgmentForPhoto(
  photoId: number,
): Promise<CuratorJudgment | null> {
  return await invoke<CuratorJudgment | null>("get_curator_judgment_for_photo", {
    photoId,
  });
}

export async function getCuratorJudgmentsForShoot(
  shootId: number,
): Promise<CuratorJudgment[]> {
  return await invoke<CuratorJudgment[]>("get_curator_judgments_for_shoot", {
    shootId,
  });
}

export async function getCuratorSummary(
  shootId: number,
): Promise<CuratorShootSummary | null> {
  return await invoke<CuratorShootSummary | null>("get_curator_summary", {
    shootId,
  });
}

export async function getCuratorAgreementStats(
  shootId: number,
): Promise<CuratorAgreementStats> {
  return await invoke<CuratorAgreementStats>("get_curator_agreement_stats", {
    shootId,
  });
}

export async function estimateCuratorCostCents(photoCount: number): Promise<number> {
  return await invoke<number>("estimate_curator_cost_cents", { photoCount });
}

/// Accept the AI suggestion for a photo. Writes flag (if applicable)
/// + user_action='accepted' atomically. Returns the new flag string.
export async function acceptCuratorSuggestion(photoId: number): Promise<string> {
  return await invoke<string>("accept_curator_suggestion", { photoId });
}

/// Record that a manual P/X disagreed with the suggestion. Idempotent;
/// silently no-op when no judgment exists for the photo.
export async function recordCuratorOverride(photoId: number): Promise<void> {
  await invoke("record_curator_override", { photoId });
}

// ---- Helpers ----

export function formatCostCents(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}
