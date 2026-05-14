use std::collections::BTreeMap;

use api::{resolve_model_alias, InputMessage};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Model pricing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelPricing {
    pub(crate) input_per_million_usd: f64,
    pub(crate) output_per_million_usd: f64,
}

pub(crate) fn estimate_session_cost_usd(
    pricing: &BTreeMap<String, ModelPricing>,
    model: &str,
    usage: &ChatUsageTotals,
) -> Option<f64> {
    let resolved = resolve_model_alias(model);
    let pricing = pricing.get(&resolved).or_else(|| pricing.get(model))?;
    let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * pricing.input_per_million_usd;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * pricing.output_per_million_usd;
    Some(input_cost + output_cost)
}

// ---------------------------------------------------------------------------
// Chat usage totals (used by both config and chat modules)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct ChatUsageTotals {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) tool_calls: u64,
    pub(crate) turns: u64,
}

// ---------------------------------------------------------------------------
// Saved chat session
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedChatSession {
    pub(crate) version: u32,
    pub(crate) cwd: String,
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) system: Option<String>,
    #[serde(default)]
    pub(crate) max_tokens: Option<u32>,
    pub(crate) use_tools: bool,
    #[serde(default)]
    pub(crate) fast_mode: bool,
    #[serde(default)]
    pub(crate) vim_mode: bool,
    #[serde(default)]
    pub(crate) messages: Vec<InputMessage>,
    #[serde(default)]
    pub(crate) input_tokens_total: u64,
    #[serde(default)]
    pub(crate) output_tokens_total: u64,
    #[serde(default)]
    pub(crate) tool_calls_total: u64,
    #[serde(default)]
    pub(crate) turns_total: u64,
    #[serde(default)]
    pub(crate) last_request_id: Option<String>,
    pub(crate) updated_at_unix_nanos: u128,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_cost_from_pricing_table() {
        let mut pricing = BTreeMap::new();
        pricing.insert(
            "gpt-5.4".to_string(),
            ModelPricing {
                input_per_million_usd: 5.0,
                output_per_million_usd: 15.0,
            },
        );
        let usage = ChatUsageTotals {
            input_tokens: 100_000,
            output_tokens: 20_000,
            tool_calls: 0,
            turns: 1,
        };

        let cost = estimate_session_cost_usd(&pricing, "gpt", &usage).expect("cost estimate");
        assert!((cost - 0.8).abs() < 1e-9);
    }
}
