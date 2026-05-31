//! Per-turn cost settlement.
//!
//! Recomputes the cache-aware USD cost for one agent turn and records it to
//! `cost_records` (the dashboard + monthly-budget source), returning the
//! formatted `$x` string for the metadata badge.
//!
//! Extracted out of the `engine_sink` bridge per the code-organization ADR
//! (2026-05-31): the bridge translates events; cost orchestration lives here. The
//! pricing math itself is the pure, unit-tested [`crate::agent::types::calculate_cost_cached`].

use crate::agent::types::{calculate_cost_cached, format_cost};
use crate::app::AppState;
use crate::cost_store;

/// Settle one turn's cost (recompute cache-aware USD + record + format).
pub trait CostService {
    /// Returns the formatted `$x` cost for the badge; records the turn to
    /// `cost_records` as a side effect.
    fn settle_turn(
        &self,
        state: &AppState,
        conversation_id: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
    ) -> String;
}

/// The production implementation: uses the model pricing table + `cost_store`.
pub struct PricingCostService;

impl CostService for PricingCostService {
    fn settle_turn(
        &self,
        state: &AppState,
        conversation_id: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
    ) -> String {
        let cost =
            calculate_cost_cached(model, input_tokens, output_tokens, cache_read_tokens);
        cost_store::record_cost(state, conversation_id, model, input_tokens, output_tokens, cost);
        format_cost(cost)
    }
}
