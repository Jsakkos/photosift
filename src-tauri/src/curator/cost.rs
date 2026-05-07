//! Cost estimation and per-shoot spend tracking. The estimator is a
//! deliberately coarse heuristic: it tells the user before-the-fact what
//! a shoot is *likely* to cost, then `actual_cost_cents_for_usage` is
//! used to track real cost as each call returns.

use crate::curator::types::Usage;

/// Initial heuristic for the import-dialog cost estimate. Calibrated for
/// Sonnet 4.6 at Jan 2026 pricing assuming ~12 photos/cluster average +
/// ~20% singletons. Refined empirically once we have ≥10 real-shoot
/// data points (see `project_curator_design_approved.md`).
pub const COST_PER_THOUSAND_PHOTOS_CENTS: u32 = 230;

/// Pre-import estimate. Returns cents.
pub fn estimate_cents_for_photo_count(photo_count: i64) -> u32 {
    if photo_count <= 0 {
        return 0;
    }
    ((photo_count as u64 * COST_PER_THOUSAND_PHOTOS_CENTS as u64) / 1000) as u32
}

/// Per-model price table in *micro-dollars per million tokens* (so we
/// can store as integers and avoid float drift over thousands of calls).
/// Source: Anthropic pricing page Jan 2026. Updated alongside any new
/// model option in `Settings::curator_model`.
struct ModelPrices {
    input_per_mtok_micro_usd: u64,
    output_per_mtok_micro_usd: u64,
    /// Cache reads are charged at 10% of input price for ephemeral cache.
    cache_read_per_mtok_micro_usd: u64,
    /// Cache writes are charged at 125% of input price.
    cache_write_per_mtok_micro_usd: u64,
}

fn prices_for(provider: &str, model: &str) -> ModelPrices {
    // All values in micro-USD per million tokens.
    match provider {
        "anthropic" => match model {
            "claude-opus-4-7" => ModelPrices {
                input_per_mtok_micro_usd: 15_000_000,
                output_per_mtok_micro_usd: 75_000_000,
                cache_read_per_mtok_micro_usd: 1_500_000,
                cache_write_per_mtok_micro_usd: 18_750_000,
            },
            "claude-haiku-4-5" => ModelPrices {
                input_per_mtok_micro_usd: 1_000_000,
                output_per_mtok_micro_usd: 5_000_000,
                cache_read_per_mtok_micro_usd: 100_000,
                cache_write_per_mtok_micro_usd: 1_250_000,
            },
            // Sonnet is the default; unknown Anthropic models also fall
            // here so we never panic on a model rename.
            _ => ModelPrices {
                input_per_mtok_micro_usd: 3_000_000,
                output_per_mtok_micro_usd: 15_000_000,
                cache_read_per_mtok_micro_usd: 300_000,
                cache_write_per_mtok_micro_usd: 3_750_000,
            },
        },
        "gemini" => match model {
            // Jan 2026 list pricing. No per-token cache row — Gemini
            // billing for context-cache hits differs but we don't use
            // that API yet.
            "gemini-2.5-pro" => ModelPrices {
                input_per_mtok_micro_usd: 1_250_000,
                output_per_mtok_micro_usd: 10_000_000,
                cache_read_per_mtok_micro_usd: 0,
                cache_write_per_mtok_micro_usd: 0,
            },
            "gemini-2.0-flash" => ModelPrices {
                input_per_mtok_micro_usd: 100_000,
                output_per_mtok_micro_usd: 400_000,
                cache_read_per_mtok_micro_usd: 0,
                cache_write_per_mtok_micro_usd: 0,
            },
            // gemini-2.5-flash is the recommended default; unknown
            // Gemini models fall through to its pricing.
            _ => ModelPrices {
                input_per_mtok_micro_usd: 300_000,
                output_per_mtok_micro_usd: 2_500_000,
                cache_read_per_mtok_micro_usd: 0,
                cache_write_per_mtok_micro_usd: 0,
            },
        },
        // Local inference is free regardless of model. Tokens are still
        // counted (the worker reports them) but `actual_cost_cents`
        // always returns 0 here.
        "local" => ModelPrices {
            input_per_mtok_micro_usd: 0,
            output_per_mtok_micro_usd: 0,
            cache_read_per_mtok_micro_usd: 0,
            cache_write_per_mtok_micro_usd: 0,
        },
        // Unknown provider: treat as Anthropic-default-priced so we
        // never silently zero out real spend due to a typo.
        _ => ModelPrices {
            input_per_mtok_micro_usd: 3_000_000,
            output_per_mtok_micro_usd: 15_000_000,
            cache_read_per_mtok_micro_usd: 300_000,
            cache_write_per_mtok_micro_usd: 3_750_000,
        },
    }
}

/// Convert a single API call's usage into cents, given the provider +
/// model in use. Rounds to the nearest cent.
pub fn actual_cost_cents_for_usage(provider: &str, model: &str, usage: &Usage) -> u32 {
    let p = prices_for(provider, model);
    let micro_usd: u64 = (usage.input_tokens as u64) * p.input_per_mtok_micro_usd / 1_000_000
        + (usage.output_tokens as u64) * p.output_per_mtok_micro_usd / 1_000_000
        + (usage.cache_read_input_tokens as u64) * p.cache_read_per_mtok_micro_usd / 1_000_000
        + (usage.cache_creation_input_tokens as u64) * p.cache_write_per_mtok_micro_usd / 1_000_000;

    // micro_usd / 10_000 = cents, with rounding.
    ((micro_usd + 5_000) / 10_000) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_scales_linearly() {
        assert_eq!(estimate_cents_for_photo_count(1000), 230);
        assert_eq!(estimate_cents_for_photo_count(500), 115);
        assert_eq!(estimate_cents_for_photo_count(0), 0);
    }

    #[test]
    fn empty_usage_costs_nothing() {
        let u = Usage::default();
        assert_eq!(actual_cost_cents_for_usage("anthropic", "claude-sonnet-4-6", &u), 0);
    }

    #[test]
    fn sonnet_typical_cluster_call_in_expected_range() {
        // Stage 2 cluster (cache hit): ~800 cached input + ~1500 fresh + ~800 output.
        let u = Usage {
            input_tokens: 1500,
            output_tokens: 800,
            cache_read_input_tokens: 800,
            cache_creation_input_tokens: 0,
        };
        let cents = actual_cost_cents_for_usage("anthropic", "claude-sonnet-4-6", &u);
        // 1500 * $3/M + 800 * $15/M + 800 * $0.30/M = 0.0045 + 0.012 + 0.00024 ≈ $0.0167 → 2c
        assert!((1..=3).contains(&cents), "got {} cents", cents);
    }

    #[test]
    fn haiku_is_cheaper_than_sonnet() {
        let u = Usage {
            input_tokens: 1500,
            output_tokens: 800,
            cache_read_input_tokens: 800,
            cache_creation_input_tokens: 0,
        };
        let s = actual_cost_cents_for_usage("anthropic", "claude-sonnet-4-6", &u);
        let h = actual_cost_cents_for_usage("anthropic", "claude-haiku-4-5", &u);
        assert!(h <= s);
    }

    #[test]
    fn gemini_flash_cheaper_than_pro() {
        let u = Usage {
            input_tokens: 2000,
            output_tokens: 800,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        };
        let flash = actual_cost_cents_for_usage("gemini", "gemini-2.5-flash", &u);
        let pro = actual_cost_cents_for_usage("gemini", "gemini-2.5-pro", &u);
        assert!(flash < pro, "flash {} should be < pro {}", flash, pro);
    }

    #[test]
    fn local_costs_zero_regardless_of_tokens() {
        let u = Usage {
            input_tokens: 100_000,
            output_tokens: 50_000,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        };
        assert_eq!(actual_cost_cents_for_usage("local", "qwen2-vl:7b", &u), 0);
        assert_eq!(actual_cost_cents_for_usage("local", "anything-goes", &u), 0);
    }
}
